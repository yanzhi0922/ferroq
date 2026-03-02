//! # ferroq-web
//!
//! Web dashboard for ferroq — provides a browser-based UI for monitoring
//! and managing the gateway.

/// Build the dashboard routes. The caller should nest under `/dashboard`.
pub fn dashboard_routes() -> axum::Router {
    use axum::response::Html;
    use axum::routing::get;

    axum::Router::new().route("/", get(|| async { Html(DASHBOARD_HTML) }))
}

/// Embedded dashboard HTML — a self-contained SPA that fetches `/health`
/// for live data. Uses only vanilla JS and CSS (no build step needed).
const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ferroq Dashboard</title>
<style>
  :root {
    --bg: #0d1117; --surface: #161b22; --border: #30363d;
    --text: #c9d1d9; --text-dim: #8b949e; --accent: #58a6ff;
    --green: #3fb950; --red: #f85149; --yellow: #d29922; --blue: #58a6ff;
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
         background: var(--bg); color: var(--text); line-height: 1.5; }
  .container { max-width: 1000px; margin: 0 auto; padding: 24px 16px; }

  /* Header */
  header { display: flex; align-items: center; gap: 12px; margin-bottom: 32px; }
  header h1 { font-size: 1.8em; font-weight: 700; }
  header h1 span { color: var(--accent); }
  .badge { font-size: 0.7em; background: var(--surface); border: 1px solid var(--border);
           border-radius: 12px; padding: 2px 10px; color: var(--text-dim); vertical-align: middle; }

  /* Stats grid */
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
           gap: 16px; margin-bottom: 32px; }
  .stat-card { background: var(--surface); border: 1px solid var(--border); border-radius: 8px;
               padding: 20px; }
  .stat-card .label { font-size: 0.85em; color: var(--text-dim); margin-bottom: 4px; }
  .stat-card .value { font-size: 2em; font-weight: 700; font-variant-numeric: tabular-nums; }

  /* Adapters table */
  .section-title { font-size: 1.2em; font-weight: 600; margin-bottom: 12px; }
  table { width: 100%; border-collapse: collapse; background: var(--surface);
          border: 1px solid var(--border); border-radius: 8px; overflow: hidden; }
  th, td { padding: 12px 16px; text-align: left; border-bottom: 1px solid var(--border); }
  th { background: rgba(255,255,255,0.03); color: var(--text-dim); font-weight: 600;
       font-size: 0.85em; text-transform: uppercase; letter-spacing: 0.05em; }
  tr:last-child td { border-bottom: none; }

  /* State badges */
  .state { display: inline-block; padding: 2px 10px; border-radius: 12px; font-size: 0.85em; font-weight: 500; }
  .state-connected { background: rgba(63,185,80,0.15); color: var(--green); }
  .state-connecting, .state-reconnecting { background: rgba(210,153,34,0.15); color: var(--yellow); }
  .state-disconnected { background: rgba(139,148,158,0.15); color: var(--text-dim); }
  .state-failed { background: rgba(248,81,73,0.15); color: var(--red); }

  /* Footer */
  .footer { margin-top: 40px; text-align: center; color: var(--text-dim); font-size: 0.85em; }
  .footer a { color: var(--accent); text-decoration: none; }
  .footer a:hover { text-decoration: underline; }

  /* Refresh indicator */
  .refresh { float: right; color: var(--text-dim); font-size: 0.8em; }
  .refresh .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%;
                  background: var(--green); margin-right: 6px; animation: pulse 2s infinite; }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.3; } }

  .error-banner { background: rgba(248,81,73,0.1); border: 1px solid var(--red);
                  border-radius: 8px; padding: 12px 16px; margin-bottom: 16px;
                  color: var(--red); display: none; }
</style>
</head>
<body>
<div class="container">
  <header>
    <h1><span>⚡</span> ferroq</h1>
    <span class="badge" id="version">v-</span>
  </header>

  <div class="error-banner" id="error-banner"></div>

  <div class="stats">
    <div class="stat-card">
      <div class="label">Uptime</div>
      <div class="value" id="uptime">-</div>
    </div>
    <div class="stat-card">
      <div class="label">Events Forwarded</div>
      <div class="value" id="events">0</div>
    </div>
    <div class="stat-card">
      <div class="label">API Calls</div>
      <div class="value" id="api-calls">0</div>
    </div>
    <div class="stat-card">
      <div class="label">WS Connections</div>
      <div class="value" id="ws-conns">0</div>
    </div>
    <div class="stat-card">
      <div class="label">Messages Stored</div>
      <div class="value" id="msgs-stored">0</div>
    </div>
    <div class="stat-card">
      <div class="label">Storage</div>
      <div class="value" id="storage-status" style="font-size:1.2em">-</div>
    </div>
  </div>

  <div style="display: flex; align-items: center; margin-bottom: 12px;">
    <div class="section-title" style="margin-bottom: 0;">Backend Adapters</div>
    <div class="refresh"><span class="dot"></span>live</div>
  </div>
  <table>
    <thead>
      <tr><th>Name</th><th>Type</th><th>URL</th><th>State</th><th>Self ID</th></tr>
    </thead>
    <tbody id="adapters-body">
      <tr><td colspan="5" style="color: var(--text-dim); text-align: center;">Loading...</td></tr>
    </tbody>
  </table>

  <div class="footer">
    <p>⚡ <a href="https://github.com/yanzhi0922/ferroq" target="_blank">ferroq</a> — High-performance QQ Bot unified gateway</p>
  </div>
</div>

<script>
function formatUptime(secs) {
  if (secs < 60) return secs + 's';
  if (secs < 3600) return Math.floor(secs/60) + 'm ' + (secs%60) + 's';
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h + 'h ' + m + 'm';
}

function formatNumber(n) {
  if (n >= 1e6) return (n/1e6).toFixed(1) + 'M';
  if (n >= 1e3) return (n/1e3).toFixed(1) + 'K';
  return n.toString();
}

function stateClass(s) {
  return 'state state-' + s;
}

async function refresh() {
  try {
    const resp = await fetch('/health');
    if (!resp.ok) throw new Error('HTTP ' + resp.status);
    const data = await resp.json();

    document.getElementById('version').textContent = 'v' + data.version;
    document.getElementById('uptime').textContent = formatUptime(data.uptime_secs);
    document.getElementById('events').textContent = formatNumber(data.events_total);
    document.getElementById('api-calls').textContent = formatNumber(data.api_calls_total);
    document.getElementById('ws-conns').textContent = data.ws_connections;
    document.getElementById('msgs-stored').textContent = formatNumber(data.messages_stored);
    document.getElementById('storage-status').textContent = data.storage_enabled ? 'Enabled' : 'Disabled';
    document.getElementById('storage-status').style.color = data.storage_enabled ? 'var(--green)' : 'var(--text-dim)';

    const tbody = document.getElementById('adapters-body');
    if (data.adapters.length === 0) {
      tbody.innerHTML = '<tr><td colspan="5" style="color: var(--text-dim); text-align: center;">No adapters configured</td></tr>';
    } else {
      tbody.innerHTML = data.adapters.map(a =>
        '<tr>' +
        '<td><strong>' + esc(a.name) + '</strong></td>' +
        '<td>' + esc(a.backend_type) + '</td>' +
        '<td style="font-size:0.9em; color:var(--text-dim)">' + esc(a.url) + '</td>' +
        '<td><span class="' + stateClass(a.state) + '">' + esc(a.state) + '</span></td>' +
        '<td>' + (a.self_id || '-') + '</td>' +
        '</tr>'
      ).join('');
    }

    document.getElementById('error-banner').style.display = 'none';
  } catch (e) {
    const banner = document.getElementById('error-banner');
    banner.textContent = 'Cannot reach /health: ' + e.message;
    banner.style.display = 'block';
  }
}

function esc(s) {
  const d = document.createElement('div');
  d.textContent = s;
  return d.innerHTML;
}

refresh();
setInterval(refresh, 2000);
</script>
</body>
</html>"##;

