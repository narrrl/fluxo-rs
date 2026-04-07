//! Human-readable help output for all available modules.
//!
//! `fluxo help` prints an overview of every module with its aliases, arguments,
//! and format tokens. `fluxo help <module>` shows the detailed page for a
//! single module.

/// Module help descriptor used to build the help output.
struct ModuleHelp {
    /// Primary display name.
    name: &'static str,
    /// CLI aliases that dispatch to this module.
    aliases: &'static [&'static str],
    /// Cargo feature gate required at compile time.
    feature: &'static str,
    /// One-line summary of what the module does.
    summary: &'static str,
    /// Argument synopsis in `[arg]` notation.
    args_synopsis: &'static str,
    /// Detailed argument descriptions.
    args_detail: &'static [(&'static str, &'static str)],
    /// Format tokens available in `config.toml`.
    tokens: &'static [(&'static str, &'static str)],
    /// Concrete usage examples.
    examples: &'static [(&'static str, &'static str)],
}

/// All module descriptors, ordered by category.
const MODULES: &[ModuleHelp] = &[
    // ── Hardware ─────────────────────────────────────────────────────
    ModuleHelp {
        name: "cpu",
        aliases: &["cpu"],
        feature: "mod-hardware",
        summary: "CPU usage percentage and temperature.",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[
            ("usage", "CPU usage as a percentage (0.0 - 100.0)"),
            ("temp", "CPU temperature in degrees Celsius"),
        ],
        examples: &[("fluxo cpu", "Show current CPU usage and temperature")],
    },
    ModuleHelp {
        name: "memory",
        aliases: &["mem", "memory"],
        feature: "mod-hardware",
        summary: "RAM usage in gigabytes with usage classification.",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[
            ("used", "Used memory in GB"),
            ("total", "Total memory in GB"),
        ],
        examples: &[("fluxo mem", "Show current RAM usage")],
    },
    ModuleHelp {
        name: "sys",
        aliases: &["sys"],
        feature: "mod-hardware",
        summary: "Uptime, load averages, and process count.",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[
            ("uptime", "Human-readable uptime (e.g. \"2d 5h\")"),
            ("load1", "1-minute load average"),
            ("load5", "5-minute load average"),
            ("load15", "15-minute load average"),
            ("procs", "Number of running processes"),
        ],
        examples: &[("fluxo sys", "Show system uptime and load")],
    },
    ModuleHelp {
        name: "gpu",
        aliases: &["gpu"],
        feature: "mod-hardware",
        summary: "GPU usage, VRAM, and temperature (AMD/NVIDIA/Intel).",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[
            ("usage", "GPU utilisation percentage"),
            ("vram_used", "Used VRAM in GB (AMD/NVIDIA)"),
            ("vram_total", "Total VRAM in GB (AMD/NVIDIA)"),
            ("temp", "GPU temperature in Celsius (AMD/NVIDIA)"),
            ("freq", "GPU frequency in MHz (Intel)"),
        ],
        examples: &[("fluxo gpu", "Show GPU stats for the detected vendor")],
    },
    ModuleHelp {
        name: "disk",
        aliases: &["disk"],
        feature: "mod-hardware",
        summary: "Filesystem usage for a given mount point.",
        args_synopsis: "[mountpoint]",
        args_detail: &[(
            "mountpoint",
            "Path to the mount point to display (default: \"/\")",
        )],
        tokens: &[
            ("mount", "The mount point path"),
            ("used", "Used space in GB"),
            ("total", "Total space in GB"),
        ],
        examples: &[
            ("fluxo disk", "Show usage of the root filesystem (/)"),
            ("fluxo disk /home", "Show usage of /home"),
        ],
    },
    ModuleHelp {
        name: "pool",
        aliases: &["pool", "btrfs"],
        feature: "mod-hardware",
        summary: "Aggregated Btrfs pool usage across all btrfs mounts.",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[
            ("used", "Total used space in GB across all btrfs mounts"),
            ("total", "Total capacity in GB across all btrfs mounts"),
        ],
        examples: &[
            ("fluxo pool", "Show combined Btrfs pool usage"),
            ("fluxo btrfs", "Same as above (alias)"),
        ],
    },
    ModuleHelp {
        name: "power",
        aliases: &["power"],
        feature: "mod-hardware",
        summary: "Battery percentage and charge state from sysfs.",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[
            ("percentage", "Battery level (0 - 100)"),
            ("icon", "State icon (varies by charge level and AC status)"),
        ],
        examples: &[("fluxo power", "Show battery status")],
    },
    ModuleHelp {
        name: "game",
        aliases: &["game"],
        feature: "mod-hardware",
        summary: "Gamemode indicator (Hyprland animation state).",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[],
        examples: &[("fluxo game", "Show whether gamemode is active")],
    },
    // ── Network ──────────────────────────────────────────────────────
    ModuleHelp {
        name: "network",
        aliases: &["net", "network"],
        feature: "mod-network",
        summary: "Primary network interface, IP, and transfer rates.",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[
            ("interface", "Active interface name (e.g. \"wlan0\")"),
            ("ip", "IPv4 address of the active interface"),
            ("rx", "Receive rate in MB/s"),
            ("tx", "Transmit rate in MB/s"),
        ],
        examples: &[("fluxo net", "Show network status and throughput")],
    },
    // ── Audio ────────────────────────────────────────────────────────
    ModuleHelp {
        name: "vol (sink)",
        aliases: &["vol"],
        feature: "mod-audio",
        summary: "PulseAudio/PipeWire output (sink) volume and controls.",
        args_synopsis: "[show|up|down|mute|cycle] [step]",
        args_detail: &[
            (
                "show",
                "Display current sink volume and mute state (default)",
            ),
            ("up", "Increase volume by <step> percent (default: 5)"),
            ("down", "Decrease volume by <step> percent (default: 5)"),
            ("mute", "Toggle mute on the default sink"),
            ("cycle", "Switch to the next available output device"),
            ("step", "Volume change increment in percent (default: 5)"),
        ],
        tokens: &[
            ("name", "Device description (truncated to 20 chars)"),
            ("icon", "Volume-level icon (changes with volume/mute)"),
            ("volume", "Current volume percentage (0 - 150)"),
        ],
        examples: &[
            ("fluxo vol", "Show current sink volume"),
            ("fluxo vol up", "Increase volume by 5%"),
            ("fluxo vol up 10", "Increase volume by 10%"),
            ("fluxo vol down 2", "Decrease volume by 2%"),
            ("fluxo vol mute", "Toggle sink mute"),
            ("fluxo vol cycle", "Switch to next output device"),
        ],
    },
    ModuleHelp {
        name: "mic (source)",
        aliases: &["mic"],
        feature: "mod-audio",
        summary: "PulseAudio/PipeWire input (source/microphone) controls.",
        args_synopsis: "[show|up|down|mute|cycle] [step]",
        args_detail: &[
            (
                "show",
                "Display current source volume and mute state (default)",
            ),
            ("up", "Increase mic volume by <step> percent (default: 5)"),
            ("down", "Decrease mic volume by <step> percent (default: 5)"),
            ("mute", "Toggle mute on the default source"),
            ("cycle", "Switch to the next available input device"),
            ("step", "Volume change increment in percent (default: 5)"),
        ],
        tokens: &[
            ("name", "Device description (truncated to 20 chars)"),
            ("icon", "Microphone icon (changes with mute state)"),
            ("volume", "Current volume percentage (0 - 150)"),
        ],
        examples: &[
            ("fluxo mic", "Show current microphone volume"),
            ("fluxo mic mute", "Toggle microphone mute"),
            ("fluxo mic up 10", "Increase mic volume by 10%"),
        ],
    },
    // ── Bluetooth ────────────────────────────────────────────────────
    ModuleHelp {
        name: "bluetooth",
        aliases: &["bt", "bluetooth"],
        feature: "mod-bt",
        summary: "Bluetooth device status, connection management, and plugin modes.",
        args_synopsis: "[show|connect|disconnect|cycle|menu|get_modes|set_mode|cycle_mode] [args...]",
        args_detail: &[
            ("show", "Display the active device's status (default)"),
            (
                "connect <mac>",
                "Connect to the device with the given MAC address",
            ),
            (
                "disconnect [mac]",
                "Disconnect the active device, or a specific MAC",
            ),
            (
                "cycle",
                "Cycle through connected devices (multi-device setups)",
            ),
            (
                "menu",
                "Open an interactive device picker (client-side, uses menu_command)",
            ),
            (
                "get_modes [mac]",
                "List available plugin modes (e.g. ANC modes for Pixel Buds)",
            ),
            (
                "set_mode <mode> [mac]",
                "Set a plugin mode on the active or specified device",
            ),
            ("cycle_mode [mac]", "Advance to the next plugin mode"),
        ],
        tokens: &[
            ("alias", "Device display name (e.g. \"Pixel Buds Pro\")"),
            ("mac", "Device MAC address"),
            ("left", "Left earbud battery (plugin, e.g. \"85%\")"),
            ("right", "Right earbud battery (plugin, e.g. \"90%\")"),
            (
                "anc",
                "ANC mode label (plugin, e.g. \"ANC\", \"Aware\", \"Off\")",
            ),
        ],
        examples: &[
            ("fluxo bt", "Show the active BT device"),
            (
                "fluxo bt connect AA:BB:CC:DD:EE:FF",
                "Connect to a specific device",
            ),
            ("fluxo bt disconnect", "Disconnect the active device"),
            ("fluxo bt menu", "Open the interactive BT device menu"),
            ("fluxo bt cycle_mode", "Toggle ANC mode on Pixel Buds"),
            ("fluxo bt set_mode aware", "Set ANC to aware mode"),
        ],
    },
    // ── D-Bus ────────────────────────────────────────────────────────
    ModuleHelp {
        name: "mpris",
        aliases: &["mpris"],
        feature: "mod-dbus",
        summary: "MPRIS media player status (artist, title, playback state).",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[
            ("artist", "Current track artist"),
            ("title", "Current track title"),
            ("album", "Current track album"),
            ("status_icon", "Playback icon (play/pause/stop glyph)"),
        ],
        examples: &[("fluxo mpris", "Show current media player status")],
    },
    ModuleHelp {
        name: "backlight",
        aliases: &["backlight"],
        feature: "mod-dbus",
        summary: "Screen brightness percentage (inotify-driven).",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[
            ("percentage", "Current brightness level (0 - 100)"),
            ("icon", "Brightness bucket icon"),
        ],
        examples: &[("fluxo backlight", "Show current screen brightness")],
    },
    ModuleHelp {
        name: "keyboard",
        aliases: &["kbd", "keyboard"],
        feature: "mod-dbus",
        summary: "Active keyboard layout (Hyprland event-driven).",
        args_synopsis: "",
        args_detail: &[],
        tokens: &[(
            "layout",
            "Active keyboard layout name (e.g. \"English (US)\")",
        )],
        examples: &[("fluxo kbd", "Show the current keyboard layout")],
    },
    ModuleHelp {
        name: "dnd",
        aliases: &["dnd"],
        feature: "mod-dbus",
        summary: "Do-Not-Disturb toggle (SwayNC signal-driven / Dunst polling).",
        args_synopsis: "[show|toggle]",
        args_detail: &[
            ("show", "Display the current DND state (default)"),
            ("toggle", "Toggle DND on/off via the notification daemon"),
        ],
        tokens: &[],
        examples: &[
            ("fluxo dnd", "Show current DND state"),
            ("fluxo dnd toggle", "Toggle Do-Not-Disturb"),
        ],
    },
];

/// Print help for all modules or a single module by name.
pub fn print_help(module: Option<&str>) {
    if let Some(name) = module {
        let found = MODULES.iter().find(|m| {
            m.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
                || m.name.eq_ignore_ascii_case(name)
        });

        match found {
            Some(m) => print_module_detail(m),
            None => {
                eprintln!("Unknown module: \"{}\"\n", name);
                eprintln!("Run `fluxo help` to see all available modules.");
                std::process::exit(1);
            }
        }
    } else {
        print_overview();
    }
}

fn print_overview() {
    println!("\x1b[1;36mfluxo\x1b[0m — high-performance daemon/client for Waybar custom modules\n");

    println!("\x1b[1mUSAGE:\x1b[0m");
    println!("    fluxo daemon [--config <path>]    Start the background daemon");
    println!("    fluxo reload                      Hot-reload the daemon config");
    println!("    fluxo <module> [args...]           Query or control a module");
    println!("    fluxo help [module]                Show this help or module details\n");

    println!("\x1b[1mCONFIGURATION:\x1b[0m");
    println!("    Config file:  $XDG_CONFIG_HOME/fluxo/config.toml");
    println!("    Format tokens in config strings use {{token}} syntax.");
    println!("    Run `fluxo help <module>` to see available tokens.\n");

    let categories: &[(&str, &[&str])] = &[
        (
            "Hardware",
            &[
                "cpu", "memory", "sys", "gpu", "disk", "pool", "power", "game",
            ],
        ),
        ("Network", &["network"]),
        ("Audio", &["vol (sink)", "mic (source)"]),
        ("Bluetooth", &["bluetooth"]),
        ("D-Bus", &["mpris", "backlight", "keyboard", "dnd"]),
    ];

    println!("\x1b[1mMODULES:\x1b[0m\n");

    for (category, names) in categories {
        println!(
            "  \x1b[1;33m{}\x1b[0m  ({})",
            category,
            feature_for_category(category)
        );
        for module_name in *names {
            if let Some(m) = MODULES.iter().find(|m| m.name == *module_name) {
                let aliases = m.aliases.join(", ");
                println!("    \x1b[1;32m{:<18}\x1b[0m {}", aliases, m.summary,);
                if !m.args_synopsis.is_empty() {
                    println!("    {:<18} args: {}", "", m.args_synopsis,);
                }
            }
        }
        println!();
    }

    println!("\x1b[1mEXAMPLES:\x1b[0m\n");
    println!("    fluxo daemon                Start the daemon");
    println!("    fluxo cpu                   Show CPU usage and temperature");
    println!("    fluxo vol up 10             Increase volume by 10%");
    println!("    fluxo bt menu               Open Bluetooth device picker");
    println!("    fluxo dnd toggle            Toggle Do-Not-Disturb");
    println!("    fluxo help vol              Show detailed help for the volume module");
    println!();
    println!("For detailed module info: \x1b[1mfluxo help <module>\x1b[0m");
}

fn print_module_detail(m: &ModuleHelp) {
    println!("\x1b[1;36mfluxo {}\x1b[0m — {}\n", m.name, m.summary);

    // Aliases
    if m.aliases.len() > 1
        || m.aliases.first() != Some(&m.name.split_whitespace().next().unwrap_or(m.name))
    {
        println!("\x1b[1mALIASES:\x1b[0m  {}", m.aliases.join(", "));
        println!();
    }

    // Feature gate
    println!("\x1b[1mFEATURE:\x1b[0m  {}", m.feature);
    println!();

    // Usage
    println!("\x1b[1mUSAGE:\x1b[0m");
    let primary = m.aliases.first().unwrap_or(&m.name);
    if m.args_synopsis.is_empty() {
        println!("    fluxo {}", primary);
    } else {
        println!("    fluxo {} {}", primary, m.args_synopsis);
    }
    println!();

    // Arguments
    if !m.args_detail.is_empty() {
        println!("\x1b[1mARGUMENTS:\x1b[0m\n");
        let max_name = m
            .args_detail
            .iter()
            .map(|(n, _)| n.len())
            .max()
            .unwrap_or(0);
        for (name, desc) in m.args_detail {
            println!(
                "    \x1b[32m{:<width$}\x1b[0m    {}",
                name,
                desc,
                width = max_name
            );
        }
        println!();
    }

    // Format tokens
    if !m.tokens.is_empty() {
        println!("\x1b[1mFORMAT TOKENS:\x1b[0m  (for use in config.toml format strings)\n");
        let max_token = m.tokens.iter().map(|(t, _)| t.len()).max().unwrap_or(0);
        for (token, desc) in m.tokens {
            let padded = format!("{{{}}}", token);
            println!(
                "    \x1b[33m{:<width$}\x1b[0m  {}",
                padded,
                desc,
                width = max_token + 2 // +2 for the braces
            );
        }
        println!();
    }

    // Examples
    if !m.examples.is_empty() {
        println!("\x1b[1mEXAMPLES:\x1b[0m\n");
        for (cmd, desc) in m.examples {
            println!("    \x1b[1m$\x1b[0m {:<34}  # {}", cmd, desc);
        }
        println!();
    }
}

fn feature_for_category(category: &str) -> &'static str {
    match category {
        "Hardware" => "mod-hardware",
        "Network" => "mod-network",
        "Audio" => "mod-audio",
        "Bluetooth" => "mod-bt",
        "D-Bus" => "mod-dbus",
        _ => "default",
    }
}
