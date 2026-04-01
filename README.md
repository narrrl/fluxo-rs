# fluxo-rs

fluxo-rs is a high-performance system metrics daemon and client designed specifically for Waybar. It replaces standard shell scripts with a compiled Rust binary that collects data via a background polling loop and serves it over a Unix socket.

## Key Features

- **Asynchronous Architecture**: Built on **Tokio**, the daemon handles concurrent IPC requests and background tasks with zero latency and minimal CPU overhead.
- **Native Library Integrations**: 
    - **Audio**: Direct `libpulse` integration for event-driven, instant volume and device updates.
    - **Bluetooth**: Native `bluer` integration for robust device monitoring.
    - **Pixel Buds Pro**: Custom native RPC implementation for real-time battery and ANC control.
    - **Network**: Native `nix` and `/proc` inspection for high-speed interface monitoring.
    - **Hyprland**: Direct IPC Unix socket communication for gamemode and animation status.
- **Circuit Breaker (Failsafe)**: Automatically detects failing modules and enters a "Cool down" state to prevent resource waste and log spam.
- **Multi-threaded Polling**: Decoupled subsystem threads ensure that a hang in one system (e.g., a slow GPU probe) never freezes your entire bar.

## Modules

| Command | Description | Tokens |
| :--- | :--- | :--- |
| `cpu` | CPU usage and temperature | `{usage}`, `{temp}`, `{model}` |
| `mem` | Memory usage | `{used}`, `{total}` |
| `net` | Network status & speeds | `{interface}`, `{ip}`, `{rx}`, `{tx}` |
| `sys` | System load and uptime | `{uptime}`, `{load1}`, `{load5}`, `{load15}`, `{procs}` |
| `disk` | Disk usage | `{mount}`, `{used}`, `{total}` |
| `pool` | Btrfs aggregate storage | `{used}`, `{total}` |
| `vol` | Audio output (sink) | `{name}`, `{volume}`, `{icon}` |
| `mic` | Audio input (source) | `{name}`, `{volume}`, `{icon}` |
| `bt` | Bluetooth status & plugins | `{alias}`, `{mac}`, `{left}`, `{right}`, `{anc}` |
| `power` | Battery and AC status | `{percentage}`, `{icon}` |
| `game` | Hyprland status | active/inactive icons |

## Setup

1. **Build**: `cargo build --release`
2. **Configure**: Create `~/.config/fluxo/config.toml` (see `example.config.toml`).
3. **Daemon**: Start `fluxo-rs daemon`. It's recommended to run this as a systemd user service.

## Waybar Configuration

To achieve zero-latency updates, use **Waybar Signals**:

```jsonc
"custom/audio": {
    "exec": "fluxo vol",
    "return-type": "json",
    "interval": 5,
    "signal": 8,
    "on-click": "fluxo audio cycle sink && pkill -RTMIN+8 waybar"
},
"custom/bluetooth": {
    "exec": "fluxo bt",
    "return-type": "json",
    "interval": 5,
    "signal": 9,
    "on-click": "fluxo bt menu && pkill -RTMIN+9 waybar",
    "on-click-right": "fluxo bt cycle_mode && pkill -RTMIN+9 waybar"
}
```

## Debugging

Start the daemon with `RUST_LOG=debug` to see detailed logs of library interactions and circuit breaker status:
```bash
RUST_LOG=debug fluxo-rs daemon
```
