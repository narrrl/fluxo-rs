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

use regex::Regex;
use std::sync::LazyLock;

pub enum TokenValue<'a> {
    Float(f64),
    Int(i64),
    String(&'a str),
}

pub fn format_template(template: &str, values: &[(&str, TokenValue)]) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\{([a-zA-Z0-9_]+)(?::([<>\^])?(\d+)?(?:\.(\d+))?)?\}").unwrap()
    });

    RE.replace_all(template, |caps: &regex::Captures| {
        let name = &caps[1];
        if let Some((_, val)) = values.iter().find(|(k, _)| *k == name) {
            let align = caps.get(2).map(|m| m.as_str()).unwrap_or(">");
            let width = caps.get(3).map(|m| m.as_str().parse::<usize>().unwrap_or(0)).unwrap_or(0);
            let precision = caps.get(4).map(|m| m.as_str().parse::<usize>().unwrap_or(0));

            match val {
                TokenValue::Float(f) => format_float(*f, align, width, precision),
                TokenValue::Int(i) => format_int(*i, align, width),
                TokenValue::String(s) => format_str(s, align, width),
            }
        } else {
            caps[0].to_string()
        }
    }).into_owned()
}

fn format_float(f: f64, align: &str, width: usize, precision: Option<usize>) -> String {
    match (align, precision) {
        ("<", Some(p)) => format!("{:<width$.p$}", f, width=width, p=p),
        ("^", Some(p)) => format!("{:^width$.p$}", f, width=width, p=p),
        (">", Some(p)) => format!("{:>width$.p$}", f, width=width, p=p),
        ("<", None) => format!("{:<width$}", f, width=width),
        ("^", None) => format!("{:^width$}", f, width=width),
        (">", None) => format!("{:>width$}", f, width=width),
        _ => format!("{}", f),
    }
}

fn format_int(i: i64, align: &str, width: usize) -> String {
    match align {
        "<" => format!("{:<width$}", i, width=width),
        "^" => format!("{:^width$}", i, width=width),
        ">" => format!("{:>width$}", i, width=width),
        _ => format!("{}", i),
    }
}

fn format_str(s: &str, align: &str, width: usize) -> String {
    match align {
        "<" => format!("{:<width$}", s, width=width),
        "^" => format!("{:^width$}", s, width=width),
        ">" => format!("{:>width$}", s, width=width),
        _ => format!("{}", s),
    }
}
