//! JSON payload returned to Waybar custom modules.

use serde::{Deserialize, Serialize};

/// A Waybar custom module return value.
///
/// Serialises to the schema Waybar's `return-type: json` expects — the
/// optional fields are omitted from the output when unset.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct WaybarOutput {
    /// Primary text shown in the bar.
    pub text: String,
    /// Tooltip text shown on hover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    /// CSS class applied to the module (for styling).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Optional 0-100 value usable by bar progress indicators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_output_serialization() {
        let output = WaybarOutput {
            text: "CPU: 50%".to_string(),
            tooltip: Some("Details".to_string()),
            class: Some("normal".to_string()),
            percentage: Some(50),
        };
        let json = serde_json::to_string(&output).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["text"], "CPU: 50%");
        assert_eq!(val["tooltip"], "Details");
        assert_eq!(val["class"], "normal");
        assert_eq!(val["percentage"], 50);
    }

    #[test]
    fn test_optional_fields_omitted() {
        let output = WaybarOutput {
            text: "test".to_string(),
            tooltip: None,
            class: None,
            percentage: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["text"], "test");
        assert!(val.get("tooltip").is_none());
        assert!(val.get("class").is_none());
        assert!(val.get("percentage").is_none());
    }

    #[test]
    fn test_partial_optional_fields() {
        let output = WaybarOutput {
            text: "test".to_string(),
            tooltip: Some("tip".to_string()),
            class: None,
            percentage: Some(75),
        };
        let json = serde_json::to_string(&output).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["tooltip"], "tip");
        assert!(val.get("class").is_none());
        assert_eq!(val["percentage"], 75);
    }
}
