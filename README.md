# fluxo-rs

fluxo-rs is a high-performance system metrics daemon and client designed specifically for waybar. it replaces standard shell scripts with a compiled rust binary that collects data via a background polling loop and serves it over a unix domain socket (/tmp/fluxo.sock).

## description

the project follows a client-server architecture:
- daemon: handles heavy lifting (polling cpu, memory, network) and stores state in memory.
- client: a thin cli wrapper that connects to the daemon's socket to retrieve formatted json for waybar.

this approach eliminates process spawning overhead and temporary file locking, resulting in near-zero cpu usage for custom modules.

## modules

- net: network interface speed (rx/tx mb/s)
- cpu: global usage percentage and package temperature
- mem: used/total ram in gigabytes
- disk: disk usage for a specific mountpoint
- pool: aggregate storage usage (e.g., btrfs)
- vol: audio output volume and device management
- mic: audio input volume and device management

## dependencies

### system
- iproute2 (for network interface discovery)
- wireplumber (for volume and mute status via wpctl)
- pulseaudio (for device description and cycling via pactl)
- lm-sensors (recommended for cpu temperature)

### rust
- cargo / rustc (edition 2024)

## setup

1. build the project:
   ```
   $ git clone https://git.narl.io/nvrl/fluxo-rs
   $ cd fluxo-rs
   $ cargo build --release
   ```

2. start the daemon:
   ```
   $ ./target/release/fluxo-rs daemon &
   ```

3. configure waybar (config.jsonc):
   ```
   "custom/cpu": {
       "exec": "/path/to/fluxo-rs cpu",
       "return-type": "json"
   }
   ```

## development

### architecture
- src/main.rs: entry point and cli argument parsing
- src/daemon.rs: uds listener and background thread orchestration
- src/ipc.rs: thin client socket communication
- src/modules/: individual metric implementation logic
- src/state.rs: shared in-memory data structures

### adding a module
1. define the state structure in state.rs
2. implement the waybarmodule trait in src/modules/
3. add the polling logic to the background thread in daemon.rs
4. register the subcommand in main.rs

### build and debug
build for release:
   ```
   $ cargo build --release
   ```

run with debug logs:
   ```
   $ RUST_LOG=debug ./target/release/fluxo-rs daemon
   ```
