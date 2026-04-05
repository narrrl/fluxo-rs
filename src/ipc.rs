//! Client/daemon Unix socket IPC.
//!
//! Requests are newline-terminated `module arg1 arg2...` lines; responses are
//! the daemon's JSON payload written until EOF.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;
use tracing::debug;

/// Returns the path to the daemon's Unix socket.
///
/// Prefers `$XDG_RUNTIME_DIR/fluxo.sock`, falling back to `/tmp/fluxo.sock`.
pub fn socket_path() -> String {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        format!("{}/fluxo.sock", dir)
    } else {
        "/tmp/fluxo.sock".to_string()
    }
}

/// Send a module invocation to the daemon and return its response body.
///
/// Blocks for up to 5 seconds waiting for the daemon to reply.
pub fn request_data(module: &str, args: &[&str]) -> anyhow::Result<String> {
    let sock = socket_path();
    debug!(module, ?args, "Connecting to daemon socket: {}", sock);
    let mut stream = UnixStream::connect(&sock)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

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
