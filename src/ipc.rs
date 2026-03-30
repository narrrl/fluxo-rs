use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;
use tracing::debug;

pub fn socket_path() -> String {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        format!("{}/fluxo.sock", dir)
    } else {
        "/tmp/fluxo.sock".to_string()
    }
}

pub fn request_data(module: &str, args: &[&str]) -> anyhow::Result<String> {
    let sock = socket_path();
    debug!(module, ?args, "Connecting to daemon socket: {}", sock);
    let mut stream = UnixStream::connect(&sock)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    // Send module and args
    let mut request = module.to_string();
    for arg in args {
        request.push(' ');
        request.push_str(arg);
    }
    request.push('\n');

    debug!("Sending IPC request: {}", request.trim());
    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    debug!("Received IPC response: {}", response);

    Ok(response)
}
