<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# RPM Packaging Notes (Fedora)

This directory contains a Fedora-oriented RPM spec and build helper for the
`sshtunnel-manager` project.

## Files

- `packaging/sshtunnel-manager.spec`: RPM spec for backend daemon, runner, CLI, GNOME Shell extension, and GUI.
- `packaging/build-rpm.sh`: helper to build a source tarball from the current tree and invoke `rpmbuild`.

## Build prerequisites (Fedora)

Install packaging/build dependencies:

```bash
sudo dnf install \
  rpm-build cargo rust gcc pkgconf-pkg-config \
  gtk4-devel libadwaita-devel \
  systemd-rpm-macros desktop-file-utils
```

Runtime dependencies pulled by the package include:

- `openssh-clients` (actual `ssh` binary used for tunnels)
- `systemd` (user service manager + unit directories)
- `gnome-shell` (Quick Settings extension host)
- `gtk4`, `libadwaita` (GUI runtime)

The Rust binaries also get automatic library dependencies from RPM's ELF dependency generation.

## Build an RPM

From the repository root:

```bash
./packaging/build-rpm.sh --version 0.1.0 --release 1
```

RPMs are written under `packaging/rpmbuild/RPMS/` by default.

## systemd user-unit deployment model (important)

This project uses **systemd user services** and is packaged **system-wide**.

Correct installation location for packaged user units:

- `/usr/lib/systemd/user/sshtunnel-backendd.service`
- `/usr/lib/systemd/user/sshtunnel@.service`

Do **not** copy these units into each user's `~/.config/systemd/user/` from an RPM package.
That would duplicate packaged files and break upgrades/uninstalls.

How users/admins enable the backend:

- Per-user (recommended):
  - `systemctl --user enable --now sshtunnel-backendd.service`
- System-wide default for all users (admin policy):
  - `sudo systemctl --global enable sshtunnel-backendd.service`

Per-profile tunnel units are still enabled/disabled by the backend daemon in each user's session using `systemctl --user`.

## Installed paths (RPM)

- Binaries: `/usr/bin/`
- systemd user units: `/usr/lib/systemd/user/`
- GNOME Shell extension: `/usr/share/gnome-shell/extensions/sshtunnel-manager@legroeder2k.com/`
- Desktop entry: `/usr/share/applications/com.legroeder2k.SshTunnelManager.Gui.desktop`
