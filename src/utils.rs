use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn show_menu(prompt: &str, items: &[String], menu_cmd: &str) -> Result<String> {
    let cmd_str = menu_cmd.replace("{prompt}", prompt);
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&cmd_str)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to spawn menu command")?;

    if let Some(mut stdin) = child.stdin.take() {
        let input = items.join("\n");
        stdin.write_all(input.as_bytes()).context("Failed to write to menu stdin")?;
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
