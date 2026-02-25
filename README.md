# SSH Tunnel Manager (GNOME / Fedora)

A GNOME-native SSH tunnel manager for Fedora Workstation (GNOME 49+), implemented primarily in Rust.

The project provides:

- A Rust backend daemon managing SSH tunnels via systemd user services
- A D-Bus API for integration
- A GNOME Shell Quick Settings extension for quick connect/disconnect
- A GTK4 + Libadwaita GUI for managing tunnel profiles

The goal is to provide a user experience similar to the built-in VPN/Wi-Fi selector in GNOME.

---

## Architecture Overview

The project is structured as a Cargo workspace.

### Components

1. **Backend daemon (Rust)**
    - Manages tunnel lifecycle
    - Exposes a D-Bus API
    - Controls systemd user services

2. **Runner helper (Rust)**
    - Invoked by systemd user units
    - Reads profile configuration
    - Constructs and executes `ssh` safely

3. **CLI (`tunnelctl`)**
    - Administrative and debugging interface
    - Talks to backend and/or systemd

4. **GNOME Shell extension (GJS)**
    - Adds a Quick Settings tile
    - Lists and toggles tunnel profiles
    - Communicates with backend via D-Bus

5. **GUI Editor (Rust, GTK4 + Libadwaita)**
    - Create/edit/delete profiles
    - Toggle autostart
    - Show status and errors

---

## Repository Layout
```
sshtunnel-manager/
│
├── AGENTS.md
├── README.md
├── Cargo.toml (workspace root)
│
├── crates/
│ ├── profile/ (shared schema + validation)
│ ├── tunnelctl/ (CLI)
│ ├── backendd/ (D-Bus daemon)
│ └── runner/ (sshtunnel-runner helper)
│
├── systemd/
│ └── sshtunnel@.service
(template unit)
│
├── gnome-extension/
│
├── gui/
│
├── packaging/
└── docs/
```

---

## Requirements

### Runtime
- Fedora Workstation 43 (GNOME 49) or later
- OpenSSH (`ssh`)
- systemd (user session)

### Development
- Rust (via rustup recommended)
- cargo
- rustfmt
- clippy
- pkg-config
- gcc / clang toolchain

For GUI development:
- gtk4-devel
- libadwaita-devel
- glib2-devel

---

## Build

From the repository root:

```bash
cargo build --workspace
```

Run tests:

```bash
cargo test --workspace
```

Lint (recommended):
```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Format:
```bash
cargo fmt --all
```
## Development Milestones

See AGENTS.md for the authoritative implementation specification.

Development order:
1. Profile schema + CLI + systemd integration
2. Backend D-Bus daemon
3. GNOME Shell Quick Settings extension
4. GTK4 GUI editor

Do not skip milestones or merge responsibilities between components.

## Profiles

Profiles are stored in:

```
~/.config/sshtunnel-manager/profiles.d/
```

Each profile is a separate JSON file:

```
<profile-id>.json
```

Schema versioning is required ("schema": 1).
The backend is the source of truth for profile interpretation and validation.

## systemd Integration

Each tunnel runs as a user-level systemd service:
```
sshtunnel@<profile-id>.service
```

The template unit file is located in:

```
systemd/sshtunnel@.service
```

Autostart is implemented via:

```
systemctl --user enable --now sshtunnel@<id>.service
```

Logs are available via:

```
journalctl --user -u sshtunnel@<id>.service
```

## D-BUS Interface
The backend daemon provides:

* Bus name: com.legroeder2k.SshTunnelManager
* Object path: /com/legroeder2k/SshTunnelManager
* Interface: com.legroeder2k.SshTunnelManager1

See AGENTS.md for method and signal definitions.
The GNOME Shell extension and GUI must use this API.

## GUI (Milestone 4)

The GUI editor lives in `gui/` and is part of the workspace.

Run it during development:

```bash
cargo run -p sshtunnel-manager-gui
```

Current GUI capabilities:
- Profile list + editor form
- Create/edit/delete profile JSONs using the shared `profile` crate validation
- Multiple local/remote forwards with bind-address support
- Autostart toggle (syncs systemd user unit enable/disable)
- Connect/disconnect controls via backend D-Bus
- Runtime status + last error line (for failed tunnels)
