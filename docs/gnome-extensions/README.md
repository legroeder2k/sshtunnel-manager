# GNOME Shell Extension Manual Test Steps (Milestone 3)

This guide covers manual testing of the GNOME Shell Quick Settings extension against the Rust D-Bus backend.

## Scope

Milestone 3 acceptance focuses on:

- Quick Settings tile appears as `SSH Tunnels`
- Submenu lists tunnel profiles
- Connect/disconnect works from the submenu
- Status updates appear quickly (target: about 1 second)
- Failures are visible as `Failed` with a short message

## Prerequisites

- Fedora Workstation with GNOME Shell 49
- This repository checked out locally
- `backendd` built and runnable (`crates/backendd`)
- At least one valid profile JSON in:
  - `~/.config/sshtunnel-manager/profiles.d/`
- Systemd user unit template installed/available (`sshtunnel@.service`)

Optional example profile in repo:

- `docs/examples/local-2222-to-remote-localhost-8080.json`

## 1. Build the backend

From the repo root:

```bash
cargo build --workspace
```

## 2. Start the backend daemon (user session)

Run the backend in a terminal:

```bash
cargo run -p backendd
```

Expected output includes the D-Bus service/interface names.

## 3. Install the extension for local testing

Create a symlink into the user GNOME Shell extensions directory:

```bash
mkdir -p ~/.local/share/gnome-shell/extensions
ln -sfn \
  "$(pwd)/gnome-extensions/sshtunnel-manager@legroeder2k.com" \
  ~/.local/share/gnome-shell/extensions/sshtunnel-manager@legroeder2k.com
```

## 4. Enable / reload the extension

Use GNOME Extensions app or CLI:

```bash
gnome-extensions enable sshtunnel-manager@legroeder2k.com
```

If the extension was already enabled and you changed code, disable and re-enable it:

```bash
gnome-extensions disable sshtunnel-manager@legroeder2k.com
gnome-extensions enable sshtunnel-manager@legroeder2k.com
```

## 5. Open Quick Settings and validate initial state

Open GNOME Quick Settings and verify:

- Tile label is `SSH Tunnels`
- Tile subtitle is one of:
  - `Loading...`
  - `Off`
  - `Connected (n)`
  - `Failed (n)`
  - `Connected (n), Failed (m)`
- Clicking the tile opens a submenu
- Submenu contains:
  - Profile rows (or `No tunnel profiles found`)
  - `Disconnect all`
  - `Open Tunnel Manager`

If the backend is not running, expected subtitle/menu state is `Backend unavailable`.

## 6. Test connect/disconnect per profile

For each available profile:

1. Toggle it on in the submenu.
2. Verify status changes to `Connecting` then `Connected` (or `Failed`) within about 1 second.
3. Toggle it off.
4. Verify status returns to `Off` / disconnected.

Cross-check from another terminal:

```bash
systemctl --user status "sshtunnel@<profile-id>.service"
```

## 7. Test `Disconnect all`

1. Connect 2 or more profiles (if available).
2. Click `Disconnect all`.
3. Verify all rows return to disconnected status.
4. Verify tile subtitle updates accordingly.

## 8. Test failure visibility

Use a deliberately broken profile (bad host, bad identity file path, or invalid target that causes `ssh` to fail).

Verify:

- Profile row shows `Failed`
- A short failure message appears in the profile row text
- Tile subtitle reflects failed count (`Failed (n)` or combined status)

## 9. Test backend restart / reconnect behavior

1. With the extension enabled, stop the backend terminal process.
2. Verify the tile eventually shows `Backend unavailable`.
3. Start the backend again.
4. Verify the extension reconnects automatically and profile list returns.

## 10. Test `Open Tunnel Manager` action (current milestone behavior)

Current Milestone 3 may not have the GUI installed yet.

Verify one of:

- If GUI desktop file exists: the app launches
- If GUI is not installed: a GNOME error notification appears

## Troubleshooting

- Inspect extension logs:

```bash
journalctl --user -f /usr/bin/gnome-shell
```

- Inspect backend logs (if running in terminal, watch stdout/stderr directly)
- Verify D-Bus service is reachable:

```bash
gdbus introspect --session \
  --dest com.legroeder2k.SshTunnelManager \
  --object-path /com/legroeder2k/SshTunnelManager
```

- Verify profiles exist:

```bash
ls -la ~/.config/sshtunnel-manager/profiles.d/
```

## Manual Test Checklist (copy into PR/review notes)

- [ ] Tile appears in Quick Settings as `SSH Tunnels`
- [ ] Submenu lists profiles
- [ ] Toggle on connects profile
- [ ] Toggle off disconnects profile
- [ ] `Disconnect all` works
- [ ] Subtitle reflects connected count
- [ ] Failed tunnels show failed status/message
- [ ] Backend restart is handled (unavailable -> reconnect)
