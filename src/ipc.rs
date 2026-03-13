use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use tracing::debug;

pub const SOCKET_PATH: &str = "/tmp/fluxo.sock";

pub fn request_data(module: &str, args: &[String]) -> anyhow::Result<String> {
    debug!(module, ?args, "Connecting to daemon socket: {}", SOCKET_PATH);
    let mut stream = UnixStream::connect(SOCKET_PATH)?;
    
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
