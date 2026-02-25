# SPDX-License-Identifier: GPL-3.0-or-later
%global srcname sshtunnel-manager
%global pkg_version %{?pkg_version}%{!?pkg_version:0.1.0}
%global pkg_release %{?pkg_release}%{!?pkg_release:1}

Name:           %{srcname}
Version:        %{pkg_version}
Release:        %{pkg_release}%{?dist}
Summary:        GNOME SSH tunnel manager with Quick Settings integration
License:        GPL-3.0-or-later
URL:            https://github.com/legroeder2k/sshtunnel-manager
Source0:        %{srcname}-%{version}.tar.gz

#BuildRequires:  cargo
#BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconfig
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1)
BuildRequires:  systemd-rpm-macros
BuildRequires:  desktop-file-utils

Requires:       openssh-clients
Requires:       systemd
Requires:       gnome-shell
Requires:       gtk4
Requires:       libadwaita

%description
SSH tunnel manager for GNOME on Fedora. It includes a Rust backend daemon,
runner helper, CLI, GNOME Shell Quick Settings extension, and a GTK4/
Libadwaita GUI editor.

The package installs per-user systemd unit files system-wide under
%{_userunitdir}. Users enable the backend service in their own user session
with `systemctl --user enable --now sshtunnel-backendd.service`.

%prep
%autosetup -n %{srcname}-%{version}

%build
cargo build --release --locked --workspace --bins

%install
install -d %{buildroot}%{_bindir}
install -pm0755 target/release/sshtunnel-manager-backendd %{buildroot}%{_bindir}/sshtunnel-manager-backendd
install -pm0755 target/release/sshtunnel-runner %{buildroot}%{_bindir}/sshtunnel-runner
install -pm0755 target/release/tunnelctl %{buildroot}%{_bindir}/tunnelctl
install -pm0755 target/release/sshtunnel-manager-gui %{buildroot}%{_bindir}/sshtunnel-manager-gui

install -d %{buildroot}%{_userunitdir}
install -pm0644 systemd/sshtunnel@.service %{buildroot}%{_userunitdir}/sshtunnel@.service
install -pm0644 systemd/sshtunnel-backendd.service %{buildroot}%{_userunitdir}/sshtunnel-backendd.service

install -d %{buildroot}%{_datadir}/gnome-shell/extensions/sshtunnel-manager@legroeder2k.com
cp -a gnome-extensions/sshtunnel-manager@legroeder2k.com/. \
  %{buildroot}%{_datadir}/gnome-shell/extensions/sshtunnel-manager@legroeder2k.com/

install -d %{buildroot}%{_datadir}/applications
install -pm0644 crates/gui/com.legroeder2k.SshTunnelManager.Gui.desktop \
  %{buildroot}%{_datadir}/applications/com.legroeder2k.SshTunnelManager.Gui.desktop

desktop-file-validate %{buildroot}%{_datadir}/applications/com.legroeder2k.SshTunnelManager.Gui.desktop

# User units are installed system-wide in %{_userunitdir}; do not copy them into
# per-user ~/.config/systemd/user/ directories from an RPM package.

%post
%systemd_user_post sshtunnel-backendd.service sshtunnel@.service

%preun
%systemd_user_preun sshtunnel-backendd.service sshtunnel@.service

%postun
%systemd_user_postun sshtunnel-backendd.service sshtunnel@.service

%files
%license LICENSE
%doc README.md docs/gnome-extensions/README.md
%{_bindir}/sshtunnel-manager-backendd
%{_bindir}/sshtunnel-runner
%{_bindir}/tunnelctl
%{_bindir}/sshtunnel-manager-gui
%{_userunitdir}/sshtunnel@.service
%{_userunitdir}/sshtunnel-backendd.service
%{_datadir}/gnome-shell/extensions/sshtunnel-manager@legroeder2k.com/
%{_datadir}/applications/com.legroeder2k.SshTunnelManager.Gui.desktop

%changelog
* Wed Feb 25 2026 legroeder2k <me@legroeder.rocks> - %{version}-%{release}
- Initial Fedora RPM packaging for backend, extension, and GUI
