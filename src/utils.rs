//! Shared helpers: menu spawning, Hyprland socket lookup, and template formatting.

use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// Pipe `items` into an external menu program (rofi/dmenu/wofi) and return
/// the user's selection.
///
/// The prompt is exposed to the command as `$FLUXO_PROMPT` (preferred) and
/// as a legacy `{prompt}` placeholder that is substituted in the shell string.
pub fn show_menu(prompt: &str, items: &[String], menu_cmd: &str) -> Result<String> {
    let cmd_str = menu_cmd.replace("{prompt}", prompt);
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&cmd_str)
        .env("FLUXO_PROMPT", prompt)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Suppress GTK/Wayland noise from tools like wofi.
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to spawn menu command")?;

    if let Some(mut stdin) = child.stdin.take() {
        let mut input = items.join("\n");
        // Trailing newline is required by wofi/rofi.
        input.push('\n');
        stdin
            .write_all(input.as_bytes())
            .context("Failed to write to menu stdin")?;
    }

    let output = child.wait_with_output().context("Failed to wait on menu")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("Menu cancelled or failed"));
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected.is_empty() {
        return Err(anyhow::anyhow!("No item selected"));
    }

    Ok(selected)
}

/// Resolve the path to a Hyprland IPC socket for the current session.
///
/// Looks under `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/` first,
/// then falls back to `/tmp/hypr/...` for older Hyprland builds.
pub fn get_hyprland_socket(socket_name: &str) -> Result<std::path::PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .context("HYPRLAND_INSTANCE_SIGNATURE not set")?;

    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let path = std::path::PathBuf::from(runtime_dir)
            .join("hypr")
            .join(&signature)
            .join(socket_name);
        if path.exists() {
            return Ok(path);
        }
    }

    let path = std::path::PathBuf::from("/tmp/hypr")
        .join(&signature)
        .join(socket_name);

    if path.exists() {
        Ok(path)
    } else {
        Err(anyhow::anyhow!(
            "Hyprland socket {} not found in runtime dir or /tmp",
            socket_name
        ))
    }
}

use regex::Regex;
use std::sync::LazyLock;

/// Bucket `value` into `"normal"`, `"high"`, or `"max"` for CSS class output.
pub fn classify_usage(value: f64, high: f64, max: f64) -> &'static str {
    if value > max {
        "max"
    } else if value > high {
        "high"
    } else {
        "normal"
    }
}

/// A typed value supplied to [`format_template`] — chosen at call site so
/// formatting handles precision and alignment correctly per type.
pub enum TokenValue {
    Float(f64),
    Int(i64),
    String(String),
}

/// Token grammar: `{name}`, `{name:>5}`, `{name:<8.2}`, `{name:^6}`, etc.
pub static TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{([a-zA-Z0-9_]+)(?::([<>\^])?(\d+)?(?:\.(\d+))?)?\}").unwrap());

/// Substitute `{name[:align[width[.precision]]]}` tokens in a template string.
///
/// Unknown tokens are left verbatim. Width/alignment/precision follow Rust's
/// `std::fmt` semantics (`<` left, `^` center, `>` right).
pub fn format_template<K>(template: &str, values: &[(K, TokenValue)]) -> String
where
    K: AsRef<str>,
{
    TOKEN_RE
        .replace_all(template, |caps: &regex::Captures| {
            let name = &caps[1];
            if let Some((_, val)) = values.iter().find(|(k, _)| k.as_ref() == name) {
                let align = caps.get(2).map(|m| m.as_str()).unwrap_or(">");
                let width = caps
                    .get(3)
                    .map(|m| m.as_str().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let precision = caps
                    .get(4)
                    .map(|m| m.as_str().parse::<usize>().unwrap_or(0));

                match val {
                    TokenValue::Float(f) => format_float(*f, align, width, precision),
                    TokenValue::Int(i) => format_int(*i, align, width),
                    TokenValue::String(s) => format_str(s, align, width),
                }
            } else {
                caps[0].to_string()
            }
        })
        .into_owned()
}

fn format_float(f: f64, align: &str, width: usize, precision: Option<usize>) -> String {
    match (align, precision) {
        ("<", Some(p)) => format!("{:<width$.p$}", f, width = width, p = p),
        ("^", Some(p)) => format!("{:^width$.p$}", f, width = width, p = p),
        (">", Some(p)) => format!("{:>width$.p$}", f, width = width, p = p),
        ("<", None) => format!("{:<width$}", f, width = width),
        ("^", None) => format!("{:^width$}", f, width = width),
        (">", None) => format!("{:>width$}", f, width = width),
        _ => format!("{}", f),
    }
}

fn format_int(i: i64, align: &str, width: usize) -> String {
    match align {
        "<" => format!("{:<width$}", i, width = width),
        "^" => format!("{:^width$}", i, width = width),
        ">" => format!("{:>width$}", i, width = width),
        _ => format!("{}", i),
    }
}

fn format_str(s: &str, align: &str, width: usize) -> String {
    match align {
        "<" => format!("{:<width$}", s, width = width),
        "^" => format!("{:^width$}", s, width = width),
        ">" => format!("{:>width$}", s, width = width),
        _ => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_string_token() {
        let result = format_template(
            "{name}",
            &[("name", TokenValue::String("hello".to_string()))],
        );
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_simple_float_token() {
        let result = format_template("{val}", &[("val", TokenValue::Float(3.15))]);
        assert_eq!(result, "3.15");
    }

    #[test]
    fn test_simple_int_token() {
        let result = format_template("{count}", &[("count", TokenValue::Int(42))]);
        assert_eq!(result, "42");
    }

    #[test]
    fn test_float_right_align_with_precision() {
        let result = format_template("{val:>8.2}", &[("val", TokenValue::Float(3.15))]);
        assert_eq!(result, "    3.15");
        assert_eq!(result.len(), 8);
    }

    #[test]
    fn test_float_left_align_with_precision() {
        let result = format_template("{val:<8.2}", &[("val", TokenValue::Float(3.15))]);
        assert_eq!(result, "3.15    ");
        assert_eq!(result.len(), 8);
    }

    #[test]
    fn test_float_center_align_with_precision() {
        let result = format_template("{val:^8.2}", &[("val", TokenValue::Float(3.15))]);
        assert_eq!(result, "  3.15  ");
        assert_eq!(result.len(), 8);
    }

    #[test]
    fn test_int_right_align() {
        let result = format_template("{val:>5}", &[("val", TokenValue::Int(42))]);
        assert_eq!(result, "   42");
    }

    #[test]
    fn test_string_left_align() {
        let result = format_template(
            "{val:<10}",
            &[("val", TokenValue::String("hi".to_string()))],
        );
        assert_eq!(result, "hi        ");
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn test_unknown_token_preserved() {
        let result = format_template(
            "{unknown}",
            &[("name", TokenValue::String("test".to_string()))],
        );
        assert_eq!(result, "{unknown}");
    }

    #[test]
    fn test_multiple_tokens() {
        let result = format_template(
            "CPU: {usage:>4.1}% {temp:>4.1}C",
            &[
                ("usage", TokenValue::Float(55.3)),
                ("temp", TokenValue::Float(65.0)),
            ],
        );
        assert_eq!(result, "CPU: 55.3% 65.0C");
    }

    #[test]
    fn test_no_tokens() {
        let result = format_template::<&'static str>("plain text", &[]);
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_empty_template() {
        let result = format_template("", &[("x", TokenValue::Int(1))]);
        assert_eq!(result, "");
    }

    #[test]
    fn test_mixed_token_types() {
        let result = format_template(
            "{name} ({ip}): {rx:>5.2} MB/s",
            &[
                ("name", TokenValue::String("eth0".to_string())),
                ("ip", TokenValue::String("10.0.0.1".to_string())),
                ("rx", TokenValue::Float(1.5)),
            ],
        );
        assert_eq!(result, "eth0 (10.0.0.1):  1.50 MB/s");
    }

    #[test]
    fn test_float_precision_zero() {
        let result = format_template("{val:>3.0}", &[("val", TokenValue::Float(99.7))]);
        assert_eq!(result, "100");
    }
}
