# AGENTS.md — Rust-first GNOME Quick Settings SSH Tunnel Manager (Fedora Workstation 43 / GNOME 49)

## Goal
Build a GNOME-native SSH tunnel manager for Fedora Workstation 43 (GNOME 49). The user can manage multiple SSH tunnel profiles and connect/disconnect them with **few clicks**, primarily via **GNOME Quick Settings**, mimicking the VPN/Wi-Fi selector UX.

The system must be robust (tunnels survive UI restarts), observable (status + logs), and safe (no password UI inside GNOME Shell).

Fedora target: Workstation 43 ships GNOME 49. :contentReference[oaicite:2]{index=2}

---

## Architecture (must follow)

### Components
1) **Backend daemon (Rust)**
   - Runs in the user session (not system-wide).
   - Owns tunnel lifecycle via **systemd user services** (preferred).
   - Exposes a **D-Bus API** for profile listing and connect/disconnect.
   - Implementation: Rust + `zbus` for D-Bus. :contentReference[oaicite:3]{index=3}

2) **Systemd user unit template**
   - One unit per profile, via a template: `sshtunnel@<profile_id>.service`.
   - Handles restart policy and logs to journal.

3) **GNOME Shell extension (Quick Settings tile)**
   - Written in GJS/JavaScript (GNOME Shell requirement).
   - Adds a Quick Settings tile + submenu list of tunnel profiles.
   - Calls the Rust backend via D-Bus.
   - Quick Settings extension pattern per GNOME docs. :contentReference[oaicite:4]{index=4}

4) **GUI editor app (Rust)**
   - Built with GTK4 + Libadwaita Rust bindings.
   - CRUD for tunnel profiles + validation.
   - Talks to backend via D-Bus (or shares a library crate for profiles + validation).
   - Libadwaita Rust binding references: gtk-rs book/docs. :contentReference[oaicite:5]{index=5}

### Non-goals (out of scope unless explicitly requested)
- No custom SSH implementation: use OpenSSH `ssh`.
- No password prompts inside GNOME Shell extension.
- No privileged/system-wide tunnels. Per-user only.
- No enterprise policy management.

---

## User experience requirements

### Quick Settings (primary UX)
- Tile label: **“SSH Tunnels”**
- Subtitle: e.g. “Connected (2)”, “Off”, “Failed (1)”
- Clicking opens a submenu with:
  - List of tunnel profiles (name + status)
  - Each item toggles connect/disconnect
  - “Disconnect all”
  - “Open Tunnel Manager” (launch GUI)

### GUI editor (secondary UX)
- App is searchable in GNOME Overview (“Tunnel Manager”, “SSH Tunnels” keywords).
- Profile list + details/edit view.
- Supports:
  - Profile name (unique)
  - Destination: user, host, port
  - Identity file (optional)
  - ProxyJump / jump host (optional)
  - Forward type: Local (-L) + Remote (-R)
  - Bind addresses (optional)
  - Multiple forwards per profile
  - Autostart at login (enable systemd unit)
- Shows last error message for failed tunnels.

---

## Functional requirements

### Tunnel capability
Support:
- Local forward: `ssh -N -L [bind_addr:]localPort:remoteHost:remotePort user@sshHost`
- Remote forward: `ssh -N -R [bind_addr:]remotePort:localHost:localPort user@sshHost`

Robustness flags (default):
- `-o ExitOnForwardFailure=yes`
- Keepalives:
  - `-o ServerAliveInterval=30`
  - `-o ServerAliveCountMax=3`

Nice-to-have (not required for MVP):
- SOCKS (-D)
- ControlMaster multiplexing

### Profile storage
- Directory: `~/.config/sshtunnel-manager/profiles.d/`
- One JSON file per profile: `<id>.json`
- Schema versioned, e.g. `"schema": 1`

### Backend D-Bus API (Rust via `zbus`)
Define a stable API:
- Bus name: `com.example.SshTunnelManager`
- Object path: `/com/example/SshTunnelManager`
- Interface: `com.example.SshTunnelManager1`

Methods:
- `ListProfiles() -> a(sssb)` (id, name, status, autostart)
- `GetProfile(id) -> s` (profile JSON)
- `Connect(id)`
- `Disconnect(id)`
- `ConnectAll()`
- `DisconnectAll()`
- `GetStatus(id) -> s`

Signals:
- `ProfileStatusChanged(id, status, message)`
- `ProfilesChanged()`

Status values (string):
- `disconnected` | `connecting` | `connected` | `failed`

Rust implementation notes:
- Use `zbus` for the service and client APIs. :contentReference[oaicite:6]{index=6}
- Consider `zbus_systemd` (pure Rust systemd D-Bus helper) if it fits; otherwise shell out to `systemctl --user` with careful escaping. :contentReference[oaicite:7]{index=7}

### systemd user services
Provide a template unit file: `sshtunnel@.service`
- `%i` is the profile id.
- ExecStart uses a backend helper binary to transform profile JSON -> ssh argv (avoid embedding JSON parsing in unit file).
- Restart on failure with backoff.
- Logs to journal.

Autostart:
- If profile `autostart=true`, backend runs:
  - `systemctl --user enable --now sshtunnel@<id>.service`
- Else:
  - `systemctl --user disable --now sshtunnel@<id>.service`

### Security
- Do not store passwords.
- Identity file path is referenced only.
- Rely on ssh-agent / GNOME Keyring / standard askpass behavior; no custom password UI.

---

## Rust technology choices (preferred)
- Workspace: `cargo` workspace with shared crates.
- Backend daemon:
  - `tokio` runtime (or async-std) + `zbus` async API.
- D-Bus:
  - `zbus` (preferred). :contentReference[oaicite:8]{index=8}
- CLI:
  - `clap` + `anyhow`/`thiserror`.
- Config:
  - `serde` + `serde_json`
  - Validation: `validator` crate or custom.
- GUI:
  - `gtk4` + `libadwaita` Rust bindings. :contentReference[oaicite:9]{index=9}

---

## Suggested build order (follow this order)

### Milestone 1 — Rust profile lib + CLI + systemd units (MVP plumbing)
Deliverables:
- `crates/profile/` (schema structs + load/save + validation)
- `crates/tunnelctl/` CLI:
  - `list`, `up <id>`, `down <id>`, `status <id>`, `logs <id>`
- `systemd/sshtunnel@.service` template
- `bin/sshtunnel-runner` (helper invoked by systemd unit):
  - reads profile JSON
  - constructs ssh argv safely
  - execs ssh

Acceptance:
- Creating a profile file and starting it works via:
  - `systemctl --user start sshtunnel@demo.service`
- Status visible via `systemctl --user status ...`
- Logs visible via `journalctl --user -u sshtunnel@demo.service`

### Milestone 2 — Rust D-Bus backend daemon
Deliverables:
- `crates/backendd/` D-Bus service:
  - exposes API above
  - wraps systemd actions
  - emits status change signals
- Status + error message:
  - Determine state from systemd ActiveState / SubState
  - For `failed`, include a short error (exit code or last log line)

Acceptance:
- `gdbus call` (or a Rust D-Bus client) can list/connect/disconnect.
- Signals fire on connect/disconnect/failure.

### Milestone 3 — GNOME Shell extension (Quick Settings)
Deliverables:
- `gnome-extension/`:
  - tile + submenu list
  - connects to `com.example.SshTunnelManager`
  - live updates via signals
  - “Open Tunnel Manager” entry
- Use GNOME Quick Settings extension patterns. :contentReference[oaicite:10]{index=10}

Acceptance:
- From Quick Settings, user can toggle any tunnel.
- Status updates within ~1s.
- Failed tunnels show as failed with a short message.

### Milestone 4 — Rust GUI editor (GTK4 + Libadwaita)
Deliverables:
- `gui/`:
  - profile list view
  - profile editor form
  - validation feedback
  - autostart toggle
  - connect/disconnect controls

Acceptance:
- Full CRUD for profiles without manual editing.
- App does not need to stay open for tunnels to persist.

---

## Packaging (Fedora)
Prefer RPM packaging:
- Package backend daemon + runner helper + CLI + systemd user unit template.
- Package GNOME Shell extension system-wide (`/usr/share/gnome-shell/extensions/<uuid>/`).
- Package GUI app + `.desktop` + icons.

Optional later:
- Flatpak for GUI only (backend + extension remain host-installed).

---

## Repo layout (recommended)
- `crates/profile/` (shared schema + validation)
- `crates/tunnelctl/` (CLI)
- `crates/backendd/` (D-Bus daemon)
- `crates/runner/` (sshtunnel-runner helper)
- `systemd/`
- `gnome-extension/`
- `gui/`
- `packaging/`
- `docs/`

---

## Testing requirements
- Unit tests for profile parsing + validation.
- Integration test script that:
  - writes a temp profile
  - starts tunnel via backend
  - observes D-Bus status changes
  - stops tunnel
- Manual GNOME checklist for tile behavior.

---

## Definition of done (MVP)
- Quick Settings shows tunnel profiles.
- Connect/disconnect works from Quick Settings.
- Status is live and accurate.
- Tunnels are systemd user services and survive UI restarts.
- Logs/failures are accessible.
- No password handling in the extension.
