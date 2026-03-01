//! Message segment types.
//!
//! Represents the parts of a QQ message in a backend-agnostic format,
//! compatible with OneBot v11 array message format.

use serde::{Deserialize, Serialize};

/// A segment of a message (text, image, at, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum MessageSegment {
    /// Plain text segment.
    #[serde(rename = "text")]
    Text { text: String },

    /// Image segment.
    #[serde(rename = "image")]
    Image {
        /// URL or file path or base64.
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },

    /// At / mention segment.
    #[serde(rename = "at")]
    At {
        /// User ID to mention, or "all" for @全体成员.
        qq: String,
    },

    /// Face / emoji segment.
    #[serde(rename = "face")]
    Face { id: String },

    /// Reply / quote segment.
    #[serde(rename = "reply")]
    Reply { id: String },

    /// Record / voice segment.
    #[serde(rename = "record")]
    Record {
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },

    /// Video segment.
    #[serde(rename = "video")]
    Video {
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },

    /// Forward message node.
    #[serde(rename = "forward")]
    Forward { id: String },

    /// JSON rich message.
    #[serde(rename = "json")]
    Json { data: String },

    /// XML rich message.
    #[serde(rename = "xml")]
    Xml { data: String },

    /// Poke action.
    #[serde(rename = "poke")]
    Poke {
        #[serde(rename = "type")]
        poke_type: String,
        id: String,
    },

    /// Unknown / pass-through segment.
    #[serde(other)]
    Unknown,
}

impl MessageSegment {
    /// Create a text segment.
    pub fn text(text: impl Into<String>) -> Self {
        MessageSegment::Text { text: text.into() }
    }

    /// Create an at segment.
    pub fn at(qq: impl Into<String>) -> Self {
        MessageSegment::At { qq: qq.into() }
    }

    /// Create an image segment from a URL.
    pub fn image(file: impl Into<String>) -> Self {
        MessageSegment::Image {
            file: file.into(),
            url: None,
        }
    }
}

/// Convert a message segment list to a CQ-code style raw string.
pub fn segments_to_raw_string(segments: &[MessageSegment]) -> String {
    let mut result = String::new();
    for seg in segments {
        match seg {
            MessageSegment::Text { text } => result.push_str(text),
            MessageSegment::At { qq } => {
                result.push_str(&format!("[CQ:at,qq={qq}]"));
            }
            MessageSegment::Image { file, .. } => {
                result.push_str(&format!("[CQ:image,file={file}]"));
            }
            MessageSegment::Face { id } => {
                result.push_str(&format!("[CQ:face,id={id}]"));
            }
            MessageSegment::Reply { id } => {
                result.push_str(&format!("[CQ:reply,id={id}]"));
            }
            _ => {
                result.push_str("[CQ:unknown]");
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_json_roundtrip() {
        let seg = MessageSegment::text("hello world");
        let json = serde_json::to_string(&seg).expect("serialize");
        assert!(json.contains("text"));
        assert!(json.contains("hello world"));
    }

    #[test]
    fn segments_to_raw() {
        let segments = vec![
            MessageSegment::text("Hello "),
            MessageSegment::at("123456"),
            MessageSegment::text(" world"),
        ];
        let raw = segments_to_raw_string(&segments);
        assert_eq!(raw, "Hello [CQ:at,qq=123456] world");
    }
}
