import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Shell from 'gi://Shell';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import {QuickMenuToggle, SystemIndicator} from 'resource:///org/gnome/shell/ui/quickSettings.js';

const BUS_NAME = 'com.legroeder2k.SshTunnelManager';
const OBJECT_PATH = '/com/legroeder2k/SshTunnelManager';
const IFACE_NAME = 'com.legroeder2k.SshTunnelManager1';
const REFRESH_INTERVAL_SECONDS = 1;

function _statusLabel(status) {
    switch (status) {
    case 'connected':
        return 'Connected';
    case 'connecting':
        return 'Connecting';
    case 'failed':
        return 'Failed';
    default:
        return 'Off';
    }
}

function _sanitizeMessage(message) {
    const text = (message ?? '').toString().replace(/\s+/g, ' ').trim();
    return text.length > 120 ? `${text.slice(0, 117)}...` : text;
}

const TunnelToggle = GObject.registerClass(
class TunnelToggle extends QuickMenuToggle {
    _init(extension) {
        super._init({
            title: 'SSH Tunnels',
            iconName: 'network-vpn-symbolic',
            toggleMode: false,
        });

        this._extension = extension;
        this._profileItems = new Map();
        this._suppressToggleSignals = false;

        this.menu.setHeader('network-vpn-symbolic', 'SSH Tunnels', 'Manage SSH tunnel profiles');

        this._profilesSection = new PopupMenu.PopupMenuSection();
        this.menu.addMenuItem(this._profilesSection);

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        this._disconnectAllItem = new PopupMenu.PopupMenuItem('Disconnect all');
        this._disconnectAllItem.connect('activate', () => {
            this._extension.disconnectAll();
        });
        this.menu.addMenuItem(this._disconnectAllItem);

        this._openManagerItem = new PopupMenu.PopupMenuItem('Open Tunnel Manager');
        this._openManagerItem.connect('activate', () => {
            this._extension.openManager();
        });
        this.menu.addMenuItem(this._openManagerItem);

        this.subtitle = 'Loading...';
        this._render();
    }

    destroy() {
        this._profileItems.clear();
        super.destroy();
    }

    _clearProfilesSection() {
        this._profilesSection.removeAll();
        this._profileItems.clear();
    }

    _createProfileItem(profile) {
        const item = new PopupMenu.PopupSwitchMenuItem(profile.name, profile.status === 'connected');
        item._profileId = profile.id;
        item.connect('toggled', (_menuItem, state) => {
            if (this._suppressToggleSignals)
                return;
            this._extension.toggleProfile(profile.id, state);
        });
        this._profileItems.set(profile.id, item);
        this._updateProfileItem(profile);
        this._profilesSection.addMenuItem(item);
    }

    _updateProfileItem(profile) {
        let item = this._profileItems.get(profile.id);
        if (!item) {
            this._createProfileItem(profile);
            item = this._profileItems.get(profile.id);
        }

        const statusText = _statusLabel(profile.status);
        let text = `${profile.name} (${statusText})`;
        if (profile.status === 'failed' && profile.message)
            text += `: ${profile.message}`;
        item.label.text = text;

        const active = profile.status === 'connected' || profile.status === 'connecting';
        this._suppressToggleSignals = true;
        item.setToggleState(active);
        this._suppressToggleSignals = false;

        item.setSensitive(!profile.busy && profile.status !== 'connecting');
    }

    _render() {
        const state = this._extension.state;
        const profiles = [...state.profiles.values()].sort((a, b) => a.name.localeCompare(b.name));
        const anyConnected = profiles.some(p => p.status === 'connected');

        this._clearProfilesSection();

        if (profiles.length === 0) {
            const emptyText = state.backendAvailable ? 'No tunnel profiles found' : 'Backend unavailable';
            const emptyItem = new PopupMenu.PopupMenuItem(emptyText, {reactive: false});
            emptyItem.setSensitive(false);
            this._profilesSection.addMenuItem(emptyItem);
        } else {
            for (const profile of profiles)
                this._updateProfileItem(profile);
        }

        // Make the tile look active like VPN/Wi-Fi when any tunnel is established.
        this.checked = anyConnected;
        this.subtitle = this._subtitleFor(state, profiles);
        this._disconnectAllItem.setSensitive(
            state.backendAvailable &&
            profiles.some(p => p.status === 'connected' || p.status === 'connecting')
        );
    }

    _subtitleFor(state, profiles) {
        if (!state.backendAvailable)
            return 'Backend unavailable';
        if (state.loading && profiles.length === 0)
            return 'Loading...';
        if (profiles.length === 0)
            return 'Off';

        const failed = profiles.filter(p => p.status === 'failed').length;
        const connected = profiles.filter(p => p.status === 'connected').length;
        const connecting = profiles.filter(p => p.status === 'connecting').length;

        if (failed > 0 && connected > 0)
            return `Connected (${connected}), Failed (${failed})`;
        if (failed > 0)
            return `Failed (${failed})`;
        if (connected > 0)
            return `Connected (${connected})`;
        if (connecting > 0)
            return `Connecting (${connecting})`;
        return 'Off';
    }
});

const TunnelIndicator = GObject.registerClass(
class TunnelIndicator extends SystemIndicator {
    _init(extension) {
        super._init();

        this._indicator = this._addIndicator();
        this._indicator.icon_name = 'network-vpn-symbolic';
        this._indicator.visible = false;

        this.quickSettingsItems.push(new TunnelToggle(extension));
    }

    get toggle() {
        return this.quickSettingsItems[0];
    }
});

export default class SshTunnelManagerExtension extends Extension {
    enable() {
        this.state = {
            backendAvailable: false,
            loading: true,
            profiles: new Map(),
        };

        this._proxy = null;
        this._proxySignalId = 0;
        this._refreshSourceId = 0;
        this._connectingProxy = false;

        this._indicator = new TunnelIndicator(this);
        Main.panel.statusArea.quickSettings.addExternalIndicator(this._indicator);

        this._connectProxy();
        this._startRefreshLoop();
    }

    disable() {
        if (this._refreshSourceId) {
            GLib.Source.remove(this._refreshSourceId);
            this._refreshSourceId = 0;
        }

        if (this._proxy && this._proxySignalId) {
            this._proxy.disconnect(this._proxySignalId);
            this._proxySignalId = 0;
        }
        this._proxy = null;
        this._connectingProxy = false;

        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }

        this.state = null;
    }

    _connectProxy() {
        if (this._connectingProxy || !this.state)
            return;
        this._connectingProxy = true;

        Gio.DBusProxy.new_for_bus(
            Gio.BusType.SESSION,
            Gio.DBusProxyFlags.NONE,
            null,
            BUS_NAME,
            OBJECT_PATH,
            IFACE_NAME,
            null,
            (source, result) => {
                try {
                    this._connectingProxy = false;
                    if (!this.state)
                        return;
                    this._proxy = Gio.DBusProxy.new_for_bus_finish(result);
                    this.state.backendAvailable = true;
                    this.state.loading = false;
                    this._proxySignalId = this._proxy.connect('g-signal', (_proxy, _sender, signalName, params) => {
                        this._onDbusSignal(signalName, params);
                    });
                    this._refreshProfiles();
                } catch (e) {
                    this._connectingProxy = false;
                    this._setBackendUnavailable(e);
                }
            }
        );
    }

    _startRefreshLoop() {
        this._refreshSourceId = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT,
            REFRESH_INTERVAL_SECONDS,
            () => {
                if (!this.state)
                    return GLib.SOURCE_REMOVE;

                if (!this._proxy) {
                    this._connectProxy();
                    this._render();
                    return GLib.SOURCE_CONTINUE;
                }

                this._refreshProfiles();
                return GLib.SOURCE_CONTINUE;
            }
        );
    }

    _setBackendUnavailable(_error) {
        this._connectingProxy = false;
        if (this._proxy && this._proxySignalId) {
            this._proxy.disconnect(this._proxySignalId);
            this._proxySignalId = 0;
        }
        this._proxy = null;
        if (!this.state)
            return;
        this.state.backendAvailable = false;
        this.state.loading = false;
        this._render();
    }

    _onDbusSignal(signalName, params) {
        if (!this.state)
            return;

        const values = params.deepUnpack();
        if (signalName === 'ProfileStatusChanged') {
            const [id, status, message] = values;
            const profile = this.state.profiles.get(id);
            if (profile) {
                profile.status = status;
                if (message)
                    profile.message = _sanitizeMessage(message);
                else if (status !== 'failed')
                    profile.message = '';
                profile.busy = false;
            } else {
                this._refreshProfiles();
                return;
            }
            this._render();
            return;
        }

        if (signalName === 'ProfilesChanged')
            this._refreshProfiles();
    }

    _refreshProfiles() {
        if (!this._proxy || !this.state)
            return;

        this._call('ListProfiles', null, ([rows]) => {
            const next = new Map();
            for (const [id, name, status, autostart] of rows) {
                const prev = this.state.profiles.get(id);
                next.set(id, {
                    id,
                    name,
                    status,
                    autostart,
                    message: prev?.message ?? '',
                    busy: prev?.busy ?? false,
                });
            }
            this.state.profiles = next;
            this.state.backendAvailable = true;
            this.state.loading = false;
            this._render();
        });
    }

    _call(method, inParams, onSuccess) {
        if (!this._proxy || !this.state)
            return;

        this._proxy.call(
            method,
            inParams,
            Gio.DBusCallFlags.NONE,
            -1,
            null,
            (_proxy, result) => {
                try {
                    const value = _proxy.call_finish(result);
                    if (!this.state)
                        return;
                    this.state.backendAvailable = true;
                    if (onSuccess)
                        onSuccess(value.deepUnpack());
                } catch (e) {
                    if (e.matches?.(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
                        return;
                    if (method === 'ListProfiles')
                        this._setBackendUnavailable(e);
                    else
                        this._handleCallError(method, e);
                } finally {
                    this._render();
                }
            }
        );
    }

    _handleCallError(method, error) {
        if (!this.state)
            return;

        const message = _sanitizeMessage(error?.message ?? String(error));
        if (method === 'Connect' || method === 'Disconnect' || method === 'DisconnectAll')
            Main.notifyError('SSH Tunnels', message || `${method} failed`);
    }

    toggleProfile(id, shouldConnect) {
        const profile = this.state?.profiles.get(id);
        if (!this._proxy || !profile)
            return;

        profile.busy = true;
        if (shouldConnect)
            profile.status = 'connecting';
        this._render();

        const params = new GLib.Variant('(s)', [id]);
        this._call(shouldConnect ? 'Connect' : 'Disconnect', params, () => {
            if (!this.state)
                return;
            const updated = this.state.profiles.get(id);
            if (updated)
                updated.busy = false;
            this._refreshProfiles();
        });
    }

    disconnectAll() {
        if (!this._proxy || !this.state)
            return;

        for (const profile of this.state.profiles.values()) {
            if (profile.status === 'connected' || profile.status === 'connecting')
                profile.busy = true;
        }
        this._render();

        this._call('DisconnectAll', null, () => {
            if (!this.state)
                return;
            for (const profile of this.state.profiles.values())
                profile.busy = false;
            this._refreshProfiles();
        });
    }

    openManager() {
        const appSystem = Shell.AppSystem.get_default();
        const appIds = [
            'com.legroeder2k.SshTunnelManager.Gui.desktop',
            'com.legroeder2k.SshTunnelManager.desktop',
            'sshtunnel-manager-gui.desktop',
            'sshtunnel-manager.desktop',
        ];

        for (const appId of appIds) {
            const app = appSystem.lookup_app(appId);
            if (app) {
                app.activate();
                return;
            }
        }

        Main.notifyError('SSH Tunnels', 'Tunnel Manager GUI is not installed yet');
    }

    _render() {
        const toggle = this._indicator?.toggle;
        if (toggle)
            toggle._render();
    }
}
