//! JSON payload returned to Waybar custom modules, plus client-side
//! output formatting utilities.

use serde::{Deserialize, Serialize};

/// Waybar renders in a proportional font — replacing normal spaces with
/// figure-spaces (U+2007) keeps column widths stable across updates.
pub const FIGURE_SPACE: char = '\u{2007}';

/// Zero-width space used as cosmetic padding around module text so Waybar
/// doesn't clip leading/trailing glyphs.
pub const ZERO_WIDTH_SPACE: char = '\u{200B}';

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

impl WaybarOutput {
    /// A blank output for disabled modules.
    pub fn disabled() -> Self {
        Self {
            text: String::new(),
            tooltip: Some("Module disabled".to_string()),
            class: Some("disabled".to_string()),
            percentage: None,
        }
    }

    /// A user-visible error with tooltip detail.
    pub fn error(message: &str) -> Self {
        Self {
            text: format!("{}Error{}", ZERO_WIDTH_SPACE, ZERO_WIDTH_SPACE),
            tooltip: Some(message.to_string()),
            class: Some("error".to_string()),
            percentage: None,
        }
    }
}

/// Apply Waybar font-stabilisation to a text string.
///
/// Replaces normal spaces with figure-spaces (unless the string contains
/// markup), and wraps in zero-width spaces for cosmetic padding.
pub fn stabilize_text(text: &str) -> String {
    let processed = if text.contains('<') {
        text.to_string()
    } else {
        text.replace(' ', &FIGURE_SPACE.to_string())
    };
    format!("{}{}{}", ZERO_WIDTH_SPACE, processed, ZERO_WIDTH_SPACE)
}

/// Process an IPC response and print Waybar-compatible JSON to stdout.
///
/// On IPC failure, prints a "Daemon offline" error output and exits
/// non-zero so Waybar surfaces the problem visually.
pub fn print_waybar_response(response: anyhow::Result<String>) {
    match response {
        Ok(json_str) => match serde_json::from_str::<serde_json::Value>(&json_str) {
            Ok(mut val) => {
                if let Some(text) = val.get("text").and_then(|t| t.as_str()) {
                    val["text"] = serde_json::Value::String(stabilize_text(text));
                }
                println!("{}", serde_json::to_string(&val).unwrap());
            }
            Err(_) => println!("{}", json_str),
        },
        Err(e) => {
            let err_out = WaybarOutput {
                text: format!(
                    "{}Daemon offline ({}){}",
                    ZERO_WIDTH_SPACE, e, ZERO_WIDTH_SPACE
                ),
                tooltip: Some(e.to_string()),
                class: Some("error".to_string()),
                percentage: None,
            };
            println!("{}", serde_json::to_string(&err_out).unwrap());
            std::process::exit(1);
        }
    }
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
