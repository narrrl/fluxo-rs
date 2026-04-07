# fluxo

`fluxo` is a high-performance system metrics daemon and client designed specifically for Waybar. It entirely replaces standard shell scripts with a compiled Rust binary that collects data via a background polling loop and serves it over a Unix socket. 

With its **100% Native, Content-Based Event-Driven Architecture**, it consumes effectively 0% CPU while idle and signals Waybar to redraw *only* when the rendered UI text or icons physically change.

## Key Features

- **100% Native Architecture**: Zero shell-outs or subprocesses. Uses `bluer` for Bluetooth, `libpulse-binding` for audio, `zbus` for MPRIS/DND, and `notify` for backlight.
- **Content-Based Event Signaling**: `fluxo` evaluates your custom configuration formats internally. It only sends a `SIGRTMIN+X` signal to Waybar if the resulting string or CSS class has actually changed, eliminating pointless re-renders from raw polling fluctuations.
- **Zero-Latency Interactions**: Direct library bindings mean that when you change your volume or connect a Bluetooth device via the CLI, the daemon updates instantly.
- **Circuit Breaker (Failsafe)**: Automatically detects failing modules and enters a "Cool down" state, preventing resource waste and log spam. Fallback caching keeps your bar looking clean even during brief failures.
- **Multi-threaded Polling**: Decoupled Tokio subsystem threads ensure that a hang in one system (e.g., a slow GPU probe) never freezes your Waybar.

## Modules

| Command | Description | Tokens |
| :--- | :--- | :--- |
| `cpu` | CPU usage and temperature | `{usage}`, `{temp}`, `{model}` |
| `mem` | Memory usage | `{used}`, `{total}` |
| `net` | Network status & speeds | `{interface}`, `{ip}`, `{rx}`, `{tx}` |
| `sys` | System load and uptime | `{uptime}`, `{load1}`, `{load5}`, `{load15}`, `{procs}` |
| `disk` | Disk usage | `{mount}`, `{used}`, `{total}` |
| `pool` | Btrfs aggregate storage | `{used}`, `{total}` |
| `gpu` | GPU usage & thermals | `{usage}`, `{vram_used}`, `{vram_total}`, `{temp}` |
| `vol` | Audio output (sink) | `{name}`, `{volume}`, `{icon}` |
| `mic` | Audio input (source) | `{name}`, `{volume}`, `{icon}` |
| `bt` | Bluetooth status & plugins | `{alias}`, `{mac}`, `{left}`, `{right}`, `{anc}` |
| `power` | Battery and AC status | `{percentage}`, `{icon}` |
| `game` | Hyprland Gamemode status | active/inactive strings |
| `mpris` | Media Player status | `{artist}`, `{title}`, `{album}`, `{status_icon}` |
| `backlight` | Display Brightness | `{percentage}`, `{icon}` |
| `kbd` | Keyboard Layout | `{layout}` |
| `dnd` | Do Not Disturb (SwayNC) | active/inactive strings |

## Installation

### From Source

```bash
cargo build --release
cp target/release/fluxo ~/.cargo/bin/
```

### Debian/Ubuntu (.deb)

```bash
cargo install cargo-deb
cargo deb
sudo dpkg -i target/debian/fluxo-rs_*.deb
```

The `.deb` package installs the binary to `/usr/bin/fluxo`, the systemd user service to `/usr/lib/systemd/user/fluxo.service`, and documentation to `/usr/share/doc/fluxo/`.

## Setup

1. **Configure**: Create `~/.config/fluxo/config.toml` (see `example.config.toml`). Ensure you map your `[signals]`.
2. **Start the daemon** via systemd (recommended) or manually:

### systemd (recommended)

If installed from the `.deb`, the service file is already in place. For manual installs:

```bash
mkdir -p ~/.config/systemd/user
cp dist/fluxo.service ~/.config/systemd/user/
```

If your binary is not at `~/.cargo/bin/fluxo`, edit the `ExecStart=` path in the service file.

Then enable and start:

```bash
systemctl --user daemon-reload
systemctl --user enable --now fluxo
```

Check status:

```bash
systemctl --user status fluxo
journalctl --user -u fluxo -f
```

### Manual

```bash
fluxo daemon
```

## Waybar Configuration

To achieve zero-latency updates and zero-polling CPU usage, set `interval: 0` on your modules and rely entirely on **Waybar Signals** mapped in your `config.toml`:

```jsonc
"custom/volume": {
    "exec": "fluxo vol",
    "return-type": "json",
    "interval": 0,
    "signal": 8, // Must match the value in config.toml [signals]
    "on-click": "fluxo vol mute",
    "on-scroll-up": "fluxo vol up 1",
    "on-scroll-down": "fluxo vol down 1",
    "on-click-right": "fluxo vol cycle"
},
"custom/bluetooth-audio": {
    "format": "{}",
    "return-type": "json",
    "exec": "fluxo bt",
    "on-click": "fluxo bt menu",
    "on-click-right": "fluxo bt cycle_mode",
    "signal": 9,
    "interval": 0,
    "tooltip": true
}
```

## Debugging

Use `--loglevel` to control log verbosity (trace, debug, info, warn, error):

```bash
fluxo daemon --loglevel debug
```

Or via the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug fluxo daemon
```

For module help and available arguments:

```bash
fluxo help          # overview of all modules
fluxo help vol      # detailed help for a specific module
```
