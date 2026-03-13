# fluxo-rs

fluxo-rs is a high-performance system metrics daemon and client designed specifically for waybar. it replaces standard shell scripts with a compiled rust binary that collects data via a background polling loop and serves it over a unix domain socket (/tmp/fluxo.sock).

## description

the project follows a client-server architecture:
- daemon: handles heavy lifting (polling cpu, memory, network, gpu) and stores state in memory.
- client: a thin cli wrapper that connects to the daemon's socket to retrieve formatted json for waybar.

this approach eliminates process spawning overhead and temporary file locking, resulting in near-zero cpu usage for custom modules.

## features

- ultra-lightweight: background polling is highly optimized (e.g., O(1) process counting).
- jitter-free: uses zero-width sentinels and figure spaces to prevent waybar from trimming padding.
- configurable: customizable output formats via toml config.
- live reload: configuration can be reloaded without restarting the daemon.
- multi-vendor gpu: native support for intel (igpu), amd, and nvidia.

## modules

| command | description | tokens |
| :--- | :--- | :--- |
| `net` | network speed (rx/tx) | `{interface}`, `{ip}`, `{rx}`, `{tx}` |
| `cpu` | cpu usage and temp | `{usage}`, `{temp}` |
| `mem` | memory usage | `{used}`, `{total}` |
| `gpu` | gpu utilization | `{usage}`, `{vram_used}`, `{vram_total}`, `{temp}` |
| `sys` | system load and uptime | `{uptime}`, `{load1}`, `{load5}`, `{load15}` |
| `disk` | disk usage (default: /) | `{mount}`, `{used}`, `{total}` |
| `pool` | aggregate storage (btrfs) | `{used}`, `{total}` |
| `vol` | audio output volume | `{percentage}`, `{icon}` |
| `mic` | audio input volume | `{percentage}`, `{icon}` |
| `bt` | bluetooth status | device name and battery |
| `buds` | pixel buds pro control | left/right battery and anc state |
| `power` | battery and ac status | `{percentage}`, `{icon}` |
| `game` | hyprland gamemode status | active/inactive icon |

## setup

1. build the project:
   ```bash
   cd fluxo-rs
   cargo build --release
   ```

2. start the daemon:
   ```bash
   ./target/release/fluxo-rs daemon &
   ```

3. configuration:
   create `~/.config/fluxo/config.toml` (see `example.config.toml` for all options).

4. waybar configuration (`config.jsonc`):
   ```json
   "custom/cpu": {
       "exec": "~/path/to/fluxo-rs cpu",
       "return-type": "json"
   }
   ```

## development

### architecture
- `src/main.rs`: entry point, cli parsing, and client-side formatting logic.
- `src/daemon.rs`:uds listener, configuration management, and polling orchestration.
- `src/ipc.rs`: unix domain socket communication logic.
- `src/modules/`: individual metric implementations.
- `src/state.rs`: shared thread-safe data structures.

### adding a module
1. add the required fields to `src/state.rs`.
2. implement the `WaybarModule` trait in a new file in `src/modules/`.
3. add polling logic to `src/modules/hardware.rs` or `src/daemon.rs`.
4. register the new subcommand in `src/main.rs` and the router in `src/daemon.rs`.

### configuration reload
the daemon can reload its configuration live:
```bash
fluxo-rs reload
```

### logs
run the daemon with debug logs for troubleshooting:
```bash
RUST_LOG=debug fluxo-rs daemon
```
