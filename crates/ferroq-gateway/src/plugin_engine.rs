//! WASM Plugin Engine.
//!
//! Loads and executes WASM plugins using wasmtime.
//! Plugins can intercept events and API calls.

#[cfg(feature = "wasm-plugins")]
mod engine {
    use std::path::Path;

    use ferroq_core::api::ApiRequest;
    use ferroq_core::config::PluginConfig;
    use ferroq_core::event::Event;
    use ferroq_core::plugin::{PluginInfo, PluginResult, wasm_exports};
    use parking_lot::RwLock;
    use tracing::{debug, error, info, warn};
    use wasmtime::*;

    /// A loaded WASM plugin instance.
    pub struct WasmPlugin {
        /// Plugin metadata.
        pub info: PluginInfo,

        /// Path to the WASM file.
        pub path: String,

        /// Whether the plugin is enabled.
        pub enabled: bool,

        /// wasmtime Store with plugin state.
        store: Store<PluginState>,

        /// wasmtime Instance.
        instance: Instance,
    }

    /// Plugin state held in the wasmtime Store.
    struct PluginState {
        /// Memory for the plugin.
        memory: Option<Memory>,

        /// Result buffer set by plugin via host function.
        result_buffer: Vec<u8>,
    }

    impl WasmPlugin {
        /// Load a plugin from a WASM file.
        pub fn load(engine: &Engine, config: &PluginConfig) -> Result<Self, String> {
            let path = &config.path;

            // Read WASM bytes
            let wasm_bytes = std::fs::read(path)
                .map_err(|e| format!("failed to read WASM file '{}': {}", path, e))?;

            // Compile module
            let module = Module::new(engine, &wasm_bytes)
                .map_err(|e| format!("failed to compile WASM '{}': {}", path, e))?;

            // Create store with plugin state
            let mut store = Store::new(
                engine,
                PluginState {
                    memory: None,
                    result_buffer: Vec::new(),
                },
            );

            // Create linker with host functions
            let mut linker = Linker::new(engine);

            // Host function: ferroq_set_result(ptr, len) - plugin writes result back
            linker
                .func_wrap(
                    "env",
                    "ferroq_set_result",
                    |mut caller: Caller<'_, PluginState>, ptr: i32, len: i32| {
                        let memory = caller
                            .data()
                            .memory
                            .ok_or_else(|| anyhow::anyhow!("memory not initialized"))?;
                        let data = memory.data(&caller);

                        let start = ptr as usize;
                        let end = start + len as usize;
                        if end > data.len() {
                            return Err(anyhow::anyhow!("out of bounds memory access"));
                        }

                        caller.data_mut().result_buffer = data[start..end].to_vec();
                        Ok(())
                    },
                )
                .map_err(|e| format!("failed to add host function: {}", e))?;

            // Host function: ferroq_log(level, ptr, len) - plugin logs message
            linker
                .func_wrap(
                    "env",
                    "ferroq_log",
                    |caller: Caller<'_, PluginState>, level: i32, ptr: i32, len: i32| {
                        let memory = caller
                            .data()
                            .memory
                            .ok_or_else(|| anyhow::anyhow!("memory not initialized"))?;
                        let data = memory.data(&caller);

                        let start = ptr as usize;
                        let end = start + len as usize;
                        if end > data.len() {
                            return Err(anyhow::anyhow!("out of bounds memory access"));
                        }

                        let msg = String::from_utf8_lossy(&data[start..end]).to_string();

                        match level {
                            0 => tracing::trace!(target: "wasm_plugin", "{}", msg),
                            1 => tracing::debug!(target: "wasm_plugin", "{}", msg),
                            2 => tracing::info!(target: "wasm_plugin", "{}", msg),
                            3 => tracing::warn!(target: "wasm_plugin", "{}", msg),
                            _ => tracing::error!(target: "wasm_plugin", "{}", msg),
                        }
                        Ok(())
                    },
                )
                .map_err(|e| format!("failed to add log function: {}", e))?;

            // Instantiate module
            let instance = linker
                .instantiate(&mut store, &module)
                .map_err(|e| format!("failed to instantiate WASM '{}': {}", path, e))?;

            // Get memory export and store it
            if let Some(memory) = instance.get_memory(&mut store, "memory") {
                store.data_mut().memory = Some(memory);
            }

            // Get plugin info
            let info = Self::call_plugin_info(&mut store, &instance, path)?;

            info!(
                plugin_name = %info.name,
                plugin_version = %info.version,
                "loaded WASM plugin"
            );

            // Initialize plugin if it has init function
            if let Some(init_fn) = instance.get_func(&mut store, wasm_exports::PLUGIN_INIT) {
                let config_json = serde_json::to_string(&config.config).unwrap_or_default();
                let config_bytes = config_json.as_bytes();

                // Allocate memory in plugin for config
                if let Some(alloc_fn) = instance.get_func(&mut store, wasm_exports::ALLOC) {
                    let alloc = alloc_fn
                        .typed::<i32, i32>(&store)
                        .map_err(|e| format!("alloc type mismatch: {}", e))?;

                    let ptr = alloc
                        .call(&mut store, config_bytes.len() as i32)
                        .map_err(|e| format!("alloc failed: {}", e))?;

                    // Write config to plugin memory
                    if let Some(memory) = store.data().memory {
                        memory
                            .write(&mut store, ptr as usize, config_bytes)
                            .map_err(|e| format!("memory write failed: {}", e))?;
                    }

                    // Call init
                    let init = init_fn
                        .typed::<(i32, i32), i32>(&store)
                        .map_err(|e| format!("init type mismatch: {}", e))?;

                    let result = init
                        .call(&mut store, (ptr, config_bytes.len() as i32))
                        .map_err(|e| format!("init failed: {}", e))?;

                    if result != 0 {
                        warn!(
                            plugin = %info.name,
                            result,
                            "plugin init returned non-zero"
                        );
                    }
                }
            }

            Ok(Self {
                info,
                path: path.clone(),
                enabled: config.enabled,
                store,
                instance,
            })
        }

        /// Call plugin's info function to get metadata.
        fn call_plugin_info(
            store: &mut Store<PluginState>,
            instance: &Instance,
            path: &str,
        ) -> Result<PluginInfo, String> {
            let info_fn = instance
                .get_func(&mut *store, wasm_exports::PLUGIN_INFO)
                .ok_or_else(|| {
                    format!(
                        "plugin '{}' missing required export '{}'",
                        path,
                        wasm_exports::PLUGIN_INFO
                    )
                })?;

            // info() returns a pointer to JSON string (null-terminated or with length prefix)
            // For simplicity, we expect the plugin to call ferroq_set_result with the info JSON
            let info = info_fn
                .typed::<(), i32>(&*store)
                .map_err(|e| format!("plugin_info type mismatch: {}", e))?;

            // Call the function
            let _ = info
                .call(&mut *store, ())
                .map_err(|e| format!("plugin_info call failed: {}", e))?;

            // Read result from buffer
            let result_json = String::from_utf8_lossy(&store.data().result_buffer).to_string();
            store.data_mut().result_buffer.clear();

            if result_json.is_empty() {
                return Err(format!("plugin '{}' returned empty info", path));
            }

            serde_json::from_str(&result_json)
                .map_err(|e| format!("plugin '{}' returned invalid info JSON: {}", path, e))
        }

        /// Process an event through this plugin.
        pub fn on_event(&mut self, event: &mut Event) -> PluginResult {
            if !self.enabled {
                return PluginResult::Continue;
            }

            let Some(on_event_fn) = self
                .instance
                .get_func(&mut self.store, wasm_exports::ON_EVENT)
            else {
                return PluginResult::Continue;
            };

            // Serialize event to JSON
            let event_json = match serde_json::to_string(event) {
                Ok(json) => json,
                Err(e) => {
                    error!(plugin = %self.info.name, error = %e, "failed to serialize event");
                    return PluginResult::Error(e.to_string());
                }
            };

            // Allocate and write event to plugin memory
            let event_bytes = event_json.as_bytes();
            let ptr = match self.alloc_and_write(event_bytes) {
                Ok(p) => p,
                Err(e) => {
                    error!(plugin = %self.info.name, error = %e, "failed to allocate event");
                    return PluginResult::Error(e);
                }
            };

            // Call on_event
            let on_event = match on_event_fn.typed::<(i32, i32), i32>(&self.store) {
                Ok(f) => f,
                Err(e) => {
                    error!(plugin = %self.info.name, error = %e, "on_event type mismatch");
                    return PluginResult::Error(e.to_string());
                }
            };

            let result_code = match on_event.call(&mut self.store, (ptr, event_bytes.len() as i32))
            {
                Ok(code) => code,
                Err(e) => {
                    error!(plugin = %self.info.name, error = %e, "on_event call failed");
                    return PluginResult::Error(e.to_string());
                }
            };

            // Check if plugin mutated the event
            let result_buffer = std::mem::take(&mut self.store.data_mut().result_buffer);
            if !result_buffer.is_empty() {
                // Plugin provided modified event
                match serde_json::from_slice::<Event>(&result_buffer) {
                    Ok(modified_event) => {
                        *event = modified_event;
                    }
                    Err(e) => {
                        warn!(plugin = %self.info.name, error = %e, "failed to parse modified event");
                    }
                }
            }

            // Free allocated memory
            self.dealloc(ptr, event_bytes.len() as i32);

            PluginResult::from_i32(result_code)
        }

        /// Process an API call through this plugin.
        pub fn on_api_call(&mut self, request: &mut ApiRequest) -> PluginResult {
            if !self.enabled {
                return PluginResult::Continue;
            }

            let Some(on_api_fn) = self
                .instance
                .get_func(&mut self.store, wasm_exports::ON_API_CALL)
            else {
                return PluginResult::Continue;
            };

            // Serialize request to JSON
            let req_json = match serde_json::to_string(request) {
                Ok(json) => json,
                Err(e) => {
                    error!(plugin = %self.info.name, error = %e, "failed to serialize request");
                    return PluginResult::Error(e.to_string());
                }
            };

            // Allocate and write request to plugin memory
            let req_bytes = req_json.as_bytes();
            let ptr = match self.alloc_and_write(req_bytes) {
                Ok(p) => p,
                Err(e) => {
                    error!(plugin = %self.info.name, error = %e, "failed to allocate request");
                    return PluginResult::Error(e);
                }
            };

            // Call on_api_call
            let on_api = match on_api_fn.typed::<(i32, i32), i32>(&self.store) {
                Ok(f) => f,
                Err(e) => {
                    error!(plugin = %self.info.name, error = %e, "on_api_call type mismatch");
                    return PluginResult::Error(e.to_string());
                }
            };

            let result_code = match on_api.call(&mut self.store, (ptr, req_bytes.len() as i32)) {
                Ok(code) => code,
                Err(e) => {
                    error!(plugin = %self.info.name, error = %e, "on_api_call call failed");
                    return PluginResult::Error(e.to_string());
                }
            };

            // Check if plugin mutated the request
            let result_buffer = std::mem::take(&mut self.store.data_mut().result_buffer);
            if !result_buffer.is_empty() {
                match serde_json::from_slice::<ApiRequest>(&result_buffer) {
                    Ok(modified_request) => {
                        *request = modified_request;
                    }
                    Err(e) => {
                        warn!(plugin = %self.info.name, error = %e, "failed to parse modified request");
                    }
                }
            }

            // Free allocated memory
            self.dealloc(ptr, req_bytes.len() as i32);

            PluginResult::from_i32(result_code)
        }

        /// Allocate memory in the plugin and write data.
        fn alloc_and_write(&mut self, data: &[u8]) -> Result<i32, String> {
            let alloc_fn = self
                .instance
                .get_func(&mut self.store, wasm_exports::ALLOC)
                .ok_or_else(|| "plugin missing alloc function".to_string())?;

            let alloc = alloc_fn
                .typed::<i32, i32>(&self.store)
                .map_err(|e| format!("alloc type mismatch: {}", e))?;

            let ptr = alloc
                .call(&mut self.store, data.len() as i32)
                .map_err(|e| format!("alloc failed: {}", e))?;

            if let Some(memory) = self.store.data().memory {
                memory
                    .write(&mut self.store, ptr as usize, data)
                    .map_err(|e| format!("memory write failed: {}", e))?;
            }

            Ok(ptr)
        }

        /// Free memory in the plugin.
        fn dealloc(&mut self, ptr: i32, len: i32) {
            if let Some(dealloc_fn) = self
                .instance
                .get_func(&mut self.store, wasm_exports::DEALLOC)
            {
                if let Ok(dealloc) = dealloc_fn.typed::<(i32, i32), ()>(&self.store) {
                    let _ = dealloc.call(&mut self.store, (ptr, len));
                }
            }
        }
    }

    /// Plugin engine managing all loaded plugins.
    pub struct PluginEngine {
        /// wasmtime Engine (shared across plugins).
        engine: Engine,

        /// Loaded plugins.
        plugins: RwLock<Vec<WasmPlugin>>,
    }

    impl PluginEngine {
        /// Create a new plugin engine.
        pub fn new() -> Result<Self, String> {
            let config = Config::new();
            let engine = Engine::new(&config)
                .map_err(|e| format!("failed to create wasmtime engine: {}", e))?;

            Ok(Self {
                engine,
                plugins: RwLock::new(Vec::new()),
            })
        }

        /// Load plugins from configuration.
        pub fn load_plugins(&self, configs: &[PluginConfig]) -> Result<(), String> {
            let mut plugins = self.plugins.write();

            for config in configs {
                if !config.enabled {
                    debug!(path = %config.path, "skipping disabled plugin");
                    continue;
                }

                if !Path::new(&config.path).exists() {
                    warn!(path = %config.path, "plugin file not found, skipping");
                    continue;
                }

                match WasmPlugin::load(&self.engine, config) {
                    Ok(plugin) => {
                        info!(
                            name = %plugin.info.name,
                            version = %plugin.info.version,
                            path = %config.path,
                            "plugin loaded"
                        );
                        plugins.push(plugin);
                    }
                    Err(e) => {
                        error!(path = %config.path, error = %e, "failed to load plugin");
                    }
                }
            }

            Ok(())
        }

        /// Get list of loaded plugins.
        pub fn list_plugins(&self) -> Vec<PluginInfo> {
            self.plugins.read().iter().map(|p| p.info.clone()).collect()
        }

        /// Process an event through all plugins.
        pub fn process_event(&self, event: &mut Event) -> PluginResult {
            let mut plugins = self.plugins.write();

            for plugin in plugins.iter_mut() {
                match plugin.on_event(event) {
                    PluginResult::Continue => continue,
                    result @ (PluginResult::Handled | PluginResult::Drop) => {
                        debug!(
                            plugin = %plugin.info.name,
                            result = ?result,
                            "plugin handled event"
                        );
                        return result;
                    }
                    PluginResult::Error(e) => {
                        warn!(
                            plugin = %plugin.info.name,
                            error = %e,
                            "plugin error on event"
                        );
                        // Continue to next plugin on error
                    }
                }
            }

            PluginResult::Continue
        }

        /// Process an API call through all plugins.
        pub fn process_api_call(&self, request: &mut ApiRequest) -> PluginResult {
            let mut plugins = self.plugins.write();

            for plugin in plugins.iter_mut() {
                match plugin.on_api_call(request) {
                    PluginResult::Continue => continue,
                    result @ (PluginResult::Handled | PluginResult::Drop) => {
                        debug!(
                            plugin = %plugin.info.name,
                            result = ?result,
                            "plugin handled API call"
                        );
                        return result;
                    }
                    PluginResult::Error(e) => {
                        warn!(
                            plugin = %plugin.info.name,
                            error = %e,
                            "plugin error on API call"
                        );
                    }
                }
            }

            PluginResult::Continue
        }

        /// Enable/disable a plugin by name.
        pub fn set_plugin_enabled(&self, name: &str, enabled: bool) -> bool {
            let mut plugins = self.plugins.write();
            for plugin in plugins.iter_mut() {
                if plugin.info.name == name {
                    plugin.enabled = enabled;
                    info!(
                        plugin = %name,
                        enabled,
                        "plugin enabled state changed"
                    );
                    return true;
                }
            }
            false
        }
    }

    impl Default for PluginEngine {
        fn default() -> Self {
            Self::new().expect("failed to create default plugin engine")
        }
    }
}

#[cfg(feature = "wasm-plugins")]
pub use engine::{PluginEngine, WasmPlugin};

// When WASM plugins are disabled, provide a no-op implementation
#[cfg(not(feature = "wasm-plugins"))]
mod noop {
    use ferroq_core::api::ApiRequest;
    use ferroq_core::config::PluginConfig;
    use ferroq_core::event::Event;
    use ferroq_core::plugin::{PluginInfo, PluginResult};

    /// No-op plugin engine when WASM feature is disabled.
    pub struct PluginEngine;

    impl PluginEngine {
        /// Create a new (no-op) plugin engine.
        pub fn new() -> Result<Self, String> {
            Ok(Self)
        }

        /// Load plugins (no-op without WASM feature).
        pub fn load_plugins(&self, _configs: &[PluginConfig]) -> Result<(), String> {
            tracing::warn!("WASM plugins disabled at compile time, ignoring plugin configuration");
            Ok(())
        }

        /// List plugins (empty without WASM feature).
        pub fn list_plugins(&self) -> Vec<PluginInfo> {
            Vec::new()
        }

        /// Process event (no-op).
        pub fn process_event(&self, _event: &mut Event) -> PluginResult {
            PluginResult::Continue
        }

        /// Process API call (no-op).
        pub fn process_api_call(&self, _request: &mut ApiRequest) -> PluginResult {
            PluginResult::Continue
        }

        /// Set plugin enabled (no-op).
        pub fn set_plugin_enabled(&self, _name: &str, _enabled: bool) -> bool {
            false
        }
    }

    impl Default for PluginEngine {
        fn default() -> Self {
            Self
        }
    }
}

#[cfg(not(feature = "wasm-plugins"))]
pub use noop::PluginEngine;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_engine_creates() {
        let engine = PluginEngine::new();
        assert!(engine.is_ok());
    }

    #[test]
    fn plugin_engine_list_empty() {
        let engine = PluginEngine::new().unwrap();
        let plugins = engine.list_plugins();
        assert!(plugins.is_empty());
    }
}
