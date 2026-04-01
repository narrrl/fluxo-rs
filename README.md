# fluxo-rs

fluxo-rs is a high-performance system metrics daemon and client designed specifically for waybar. It replaces standard shell scripts with a compiled rust binary that collects data via a background polling loop and serves it over a unix domain socket (`/tmp/fluxo.sock`).

## Description

The project follows a client-server architecture:
- **Daemon**: Handles heavy lifting (polling cpu, memory, network, gpu) and stores state in memory.
- **Client**: A thin cli wrapper that connects to the daemon's socket to retrieve formatted json for waybar.

This approach eliminates process spawning overhead and temporary file locking, resulting in near-zero cpu usage for custom modules.

## Features

- **Ultra-lightweight**: Background polling is highly optimized (e.g., O(1) process counting).
- **Jitter-free**: Uses zero-width sentinels and figure spaces to prevent waybar from trimming padding.
- **Configurable**: Fully customizable output formats via toml config.
- **Interactive Menus**: Integrated support for selecting items (like Bluetooth devices) via external menus (e.g., Rofi, Wofi, Fuzzel).
- **Live Reload**: Configuration can be reloaded without restarting the daemon.
- **Multi-vendor GPU**: Native support for intel (igpu), amd, and nvidia.

## Modules

| Command | Description | Tokens |
| :--- | :--- | :--- |
| `net` | Network speed (rx/tx) | `{interface}`, `{ip}`, `{rx}`, `{tx}` |
| `cpu` | CPU usage and temp | `{usage}`, `{temp}` |
| `mem` | Memory usage | `{used}`, `{total}` |
| `gpu` | GPU utilization | `{usage}`, `{vram_used}`, `{vram_total}`, `{temp}` |
| `sys` | System load and uptime | `{uptime}`, `{load1}`, `{load5}`, `{load15}` |
| `disk` | Disk usage (default: /) | `{mount}`, `{used}`, `{total}` |
| `pool` | Aggregate storage (btrfs) | `{used}`, `{total}` |
| `vol` | Audio output volume | `{name}`, `{volume}`, `{icon}` |
| `mic` | Audio input volume | `{name}`, `{volume}`, `{icon}` |
| `bt` | Bluetooth status & plugins | `{alias}`, `{mac}`, `{left}`, `{right}`, `{anc}` |
| `power` | Battery and AC status | `{percentage}`, `{icon}` |
| `game` | Hyprland gamemode status | active/inactive icon strings |

## Setup

1. Build the project:
   ```bash
   cd fluxo-rs
   cargo build --release
   ```

2. Start the daemon:
   ```bash
   # Starts the daemon using the default config path (~/.config/fluxo/config.toml)
   ./target/release/fluxo-rs daemon &
   
   # Or provide a custom path
   ./target/release/fluxo-rs daemon --config /path/to/your/config.toml &
   ```

3. Configuration:
   Create `~/.config/fluxo/config.toml` (see `example.config.toml` for all default options).

4. Waybar configuration (`config.jsonc`):
   ```json
   "custom/cpu": {
       "exec": "~/path/to/fluxo-rs cpu",
       "return-type": "json"
   }
   ```

## Development

### Architecture
- `src/main.rs`: Entry point, CLI parsing, interactive GUI spawns (menus), and client-side formatting logic.
- `src/daemon.rs`: UDS listener, configuration management, and polling orchestration.
- `src/ipc.rs`: Unix domain socket communication logic.
- `src/utils.rs`: Generic GUI utilities (like the menu spawner).
- `src/modules/`: Individual metric implementations.
- `src/state.rs`: Shared thread-safe data structures.

### Adding a Module
1. Add the required config block to `src/config.rs`.
2. Add the required state fields to `src/state.rs`.
3. Implement the `WaybarModule` trait in a new file in `src/modules/`.
4. Add polling logic to `src/modules/hardware.rs` or `src/daemon.rs`.
5. Register the new subcommand in `src/main.rs` and the router in `src/daemon.rs`.

### Configuration Reload
The daemon can reload its configuration live:
```bash
fluxo-rs reload
```

### Logging & Debugging
`fluxo-rs` uses the `tracing` ecosystem. If a module isn't behaving properly or your configuration isn't applying, start the daemon with debug logging enabled in the foreground:
```bash
RUST_LOG=debug fluxo-rs daemon
```
This will print verbose information regarding config parsing errors, subprocess failures, and IPC requests.
