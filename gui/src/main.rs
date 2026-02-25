use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use adw::prelude::*;
use anyhow::{Context, Result, anyhow};
use glib::ControlFlow;
use profile::{Destination, Forward, LocalForward, Profile, RemoteForward, SCHEMA_VERSION};

const APP_ID: &str = "com.legroeder2k.SshTunnelManager.Gui";
const BUS_NAME: &str = "com.legroeder2k.SshTunnelManager";
const OBJECT_PATH: &str = "/com/legroeder2k/SshTunnelManager";
const IFACE_NAME: &str = "com.legroeder2k.SshTunnelManager1";

#[derive(Debug, Clone)]
struct RuntimeProfile {
    id: String,
    name: String,
    status: String,
    autostart: bool,
    last_error: String,
}

#[derive(Debug, Clone, Default)]
struct RefreshSnapshot {
    backend_available: bool,
    profiles: Vec<RuntimeProfile>,
}

#[derive(Debug)]
enum UiMsg {
    RefreshFinished(Result<RefreshSnapshot, String>),
    ActionFinished {
        label: String,
        result: Result<(), String>,
    },
}

struct ForwardRowWidgets {
    row_id: u64,
    row: gtk::ListBoxRow,
    kind: gtk::ComboBoxText,
    bind_entry: gtk::Entry,
    port1_label: gtk::Label,
    port1_spin: gtk::SpinButton,
    host_label: gtk::Label,
    host_entry: gtk::Entry,
    port2_label: gtk::Label,
    port2_spin: gtk::SpinButton,
}

struct AppUi {
    window: adw::ApplicationWindow,
    profile_list: gtk::ListBox,
    backend_status_label: gtk::Label,
    form_title_label: gtk::Label,
    validation_label: gtk::Label,
    runtime_status_label: gtk::Label,
    runtime_error_label: gtk::Label,
    id_entry: gtk::Entry,
    name_entry: gtk::Entry,
    user_entry: gtk::Entry,
    host_entry: gtk::Entry,
    ssh_port_spin: gtk::SpinButton,
    identity_entry: gtk::Entry,
    proxy_jump_entry: gtk::Entry,
    autostart_switch: gtk::Switch,
    forwards_list: gtk::ListBox,
    save_button: gtk::Button,
    delete_button: gtk::Button,
    connect_button: gtk::Button,
    disconnect_button: gtk::Button,
    new_button: gtk::Button,
    reload_button: gtk::Button,
    quit_button: gtk::Button,
    add_local_button: gtk::Button,
    add_remote_button: gtk::Button,
    sender: mpsc::Sender<UiMsg>,
    receiver: RefCell<mpsc::Receiver<UiMsg>>,
    runtime_profiles: RefCell<Vec<RuntimeProfile>>,
    selected_id: RefCell<Option<String>>,
    forward_rows: RefCell<Vec<ForwardRowWidgets>>,
    next_forward_row_id: Cell<u64>,
    refresh_in_flight: Cell<bool>,
}

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        let ui = AppUi::build(app);
        ui.start();
    });
    app.run();
}

impl AppUi {
    fn build(app: &adw::Application) -> Rc<Self> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("SSH Tunnel Manager")
            .default_width(1200)
            .default_height(760)
            .build();

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window.set_content(Some(&root));

        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        toolbar.set_margin_top(12);
        toolbar.set_margin_bottom(12);
        toolbar.set_margin_start(12);
        toolbar.set_margin_end(12);
        root.append(&toolbar);

        let new_button = gtk::Button::with_label("New Profile");
        let reload_button = gtk::Button::with_label("Reload");
        let quit_button = gtk::Button::with_label("Quit");
        toolbar.append(&new_button);
        toolbar.append(&reload_button);
        toolbar.append(&quit_button);

        let backend_status_label = gtk::Label::new(Some("Backend: loading..."));
        backend_status_label.set_xalign(1.0);
        backend_status_label.set_hexpand(true);
        toolbar.append(&backend_status_label);

        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_wide_handle(true);
        paned.set_position(340);
        root.append(&paned);

        let left_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        left_box.set_margin_start(12);
        left_box.set_margin_end(6);
        left_box.set_margin_bottom(12);
        paned.set_start_child(Some(&left_box));

        let left_title = gtk::Label::new(Some("Profiles"));
        left_title.add_css_class("title-4");
        left_title.set_xalign(0.0);
        left_box.append(&left_title);

        let profile_list = gtk::ListBox::new();
        profile_list.set_selection_mode(gtk::SelectionMode::Single);
        profile_list.add_css_class("boxed-list");
        let profile_scroller = gtk::ScrolledWindow::new();
        profile_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        profile_scroller.set_vexpand(true);
        profile_scroller.set_child(Some(&profile_list));
        left_box.append(&profile_scroller);

        let right_scroller = gtk::ScrolledWindow::new();
        right_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        right_scroller.set_vexpand(true);
        right_scroller.set_hexpand(true);
        paned.set_end_child(Some(&right_scroller));

        let form_outer = gtk::Box::new(gtk::Orientation::Vertical, 12);
        form_outer.set_margin_start(12);
        form_outer.set_margin_end(12);
        form_outer.set_margin_bottom(12);
        right_scroller.set_child(Some(&form_outer));

        let form_title_label = gtk::Label::new(Some("Create or select a profile"));
        form_title_label.add_css_class("title-3");
        form_title_label.set_xalign(0.0);
        form_title_label.set_margin_top(12);
        form_outer.append(&form_title_label);

        let validation_label = gtk::Label::new(None);
        validation_label.set_wrap(true);
        validation_label.set_xalign(0.0);
        validation_label.add_css_class("error");
        form_outer.append(&validation_label);

        let runtime_status_label = gtk::Label::new(Some("Status: disconnected"));
        runtime_status_label.set_xalign(0.0);
        form_outer.append(&runtime_status_label);

        let runtime_error_label = gtk::Label::new(None);
        runtime_error_label.set_xalign(0.0);
        runtime_error_label.set_wrap(true);
        runtime_error_label.add_css_class("dim-label");
        form_outer.append(&runtime_error_label);

        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        form_outer.append(&controls);

        let save_button = gtk::Button::with_label("Save");
        save_button.add_css_class("suggested-action");
        let delete_button = gtk::Button::with_label("Delete");
        delete_button.add_css_class("destructive-action");
        let connect_button = gtk::Button::with_label("Connect");
        let disconnect_button = gtk::Button::with_label("Disconnect");
        controls.append(&save_button);
        controls.append(&delete_button);
        controls.append(&connect_button);
        controls.append(&disconnect_button);

        let grid = gtk::Grid::new();
        grid.set_column_spacing(12);
        grid.set_row_spacing(8);
        form_outer.append(&grid);

        let id_entry = gtk::Entry::new();
        id_entry.set_placeholder_text(Some("e.g. demo-db"));
        let name_entry = gtk::Entry::new();
        let user_entry = gtk::Entry::new();
        let host_entry = gtk::Entry::new();
        let ssh_port_spin = gtk::SpinButton::with_range(1.0, 65535.0, 1.0);
        ssh_port_spin.set_value(22.0);
        let identity_entry = gtk::Entry::new();
        identity_entry.set_placeholder_text(Some("/home/user/.ssh/id_ed25519"));
        let proxy_jump_entry = gtk::Entry::new();
        proxy_jump_entry.set_placeholder_text(Some("jump.example.com or user@jump:22"));
        let autostart_switch = gtk::Switch::new();

        attach_labeled(&grid, 0, "Profile ID", &id_entry);
        attach_labeled(&grid, 1, "Name", &name_entry);
        attach_labeled(&grid, 2, "SSH User", &user_entry);
        attach_labeled(&grid, 3, "SSH Host", &host_entry);
        attach_labeled(&grid, 4, "SSH Port", &ssh_port_spin);
        attach_labeled(&grid, 5, "Identity File", &identity_entry);
        attach_labeled(&grid, 6, "ProxyJump", &proxy_jump_entry);
        attach_labeled(&grid, 7, "Autostart", &autostart_switch);

        let forwards_header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        forwards_header.set_margin_top(8);
        form_outer.append(&forwards_header);

        let forwards_title = gtk::Label::new(Some("Forwards"));
        forwards_title.add_css_class("title-5");
        forwards_title.set_xalign(0.0);
        forwards_title.set_hexpand(true);
        forwards_header.append(&forwards_title);

        let add_local_button = gtk::Button::with_label("Add Local");
        let add_remote_button = gtk::Button::with_label("Add Remote");
        forwards_header.append(&add_local_button);
        forwards_header.append(&add_remote_button);

        let forwards_list = gtk::ListBox::new();
        forwards_list.add_css_class("boxed-list");
        let forwards_scroller = gtk::ScrolledWindow::new();
        forwards_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        forwards_scroller.set_min_content_height(260);
        forwards_scroller.set_child(Some(&forwards_list));
        form_outer.append(&forwards_scroller);

        let (sender, receiver) = mpsc::channel();

        let ui = Rc::new(Self {
            window,
            profile_list,
            backend_status_label,
            form_title_label,
            validation_label,
            runtime_status_label,
            runtime_error_label,
            id_entry,
            name_entry,
            user_entry,
            host_entry,
            ssh_port_spin,
            identity_entry,
            proxy_jump_entry,
            autostart_switch,
            forwards_list,
            save_button,
            delete_button,
            connect_button,
            disconnect_button,
            new_button,
            reload_button,
            quit_button,
            add_local_button,
            add_remote_button,
            sender,
            receiver: RefCell::new(receiver),
            runtime_profiles: RefCell::new(Vec::new()),
            selected_id: RefCell::new(None),
            forward_rows: RefCell::new(Vec::new()),
            next_forward_row_id: Cell::new(1),
            refresh_in_flight: Cell::new(false),
        });

        ui.install_handlers();
        ui.reset_editor_for_new_profile();
        ui
    }

    fn start(self: &Rc<Self>) {
        // Keep the controller alive for as long as the window exists.
        let keepalive = self.clone();
        self.window.connect_close_request(move |_| {
            let _ = &keepalive;
            glib::Propagation::Proceed
        });

        self.window.present();
        self.schedule_message_pump();
        self.schedule_refresh_loop();
        self.request_refresh();
    }

    fn install_handlers(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.profile_list.connect_row_selected(move |_list, row| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let Some(row) = row else {
                return;
            };
            let id = row.widget_name().to_string();
            if id.is_empty() {
                return;
            }
            ui.load_profile_into_editor(&id);
        });

        let weak = Rc::downgrade(self);
        self.new_button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.clear_validation();
                ui.reset_editor_for_new_profile();
                ui.profile_list.unselect_all();
            }
        });

        let weak = Rc::downgrade(self);
        self.reload_button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.request_refresh();
            }
        });

        let weak = Rc::downgrade(self);
        self.quit_button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.window.close();
            }
        });

        let weak = Rc::downgrade(self);
        self.add_local_button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.add_forward_row(Some(Forward::Local(LocalForward {
                    bind_address: Some("127.0.0.1".into()),
                    local_port: 8080,
                    remote_host: "localhost".into(),
                    remote_port: 8080,
                })));
            }
        });

        let weak = Rc::downgrade(self);
        self.add_remote_button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.add_forward_row(Some(Forward::Remote(RemoteForward {
                    bind_address: None,
                    remote_port: 9000,
                    local_host: "127.0.0.1".into(),
                    local_port: 9000,
                })));
            }
        });

        let weak = Rc::downgrade(self);
        self.save_button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.save_current_profile();
            }
        });

        let weak = Rc::downgrade(self);
        self.delete_button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.delete_selected_profile();
            }
        });

        let weak = Rc::downgrade(self);
        self.connect_button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.invoke_backend_action("Connect", "Connect", ui.current_editor_id_or_selected());
            }
        });

        let weak = Rc::downgrade(self);
        self.disconnect_button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.invoke_backend_action(
                    "Disconnect",
                    "Disconnect",
                    ui.current_editor_id_or_selected(),
                );
            }
        });
    }

    fn schedule_message_pump(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(75), move || {
            let Some(ui) = weak.upgrade() else {
                return ControlFlow::Break;
            };

            loop {
                let msg = {
                    let rx = ui.receiver.borrow();
                    rx.try_recv().ok()
                };
                let Some(msg) = msg else {
                    break;
                };
                ui.handle_msg(msg);
            }

            ControlFlow::Continue
        });
    }

    fn schedule_refresh_loop(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::timeout_add_seconds_local(1, move || {
            let Some(ui) = weak.upgrade() else {
                return ControlFlow::Break;
            };
            ui.request_refresh();
            ControlFlow::Continue
        });
    }

    fn handle_msg(self: &Rc<Self>, msg: UiMsg) {
        match msg {
            UiMsg::RefreshFinished(result) => {
                self.refresh_in_flight.set(false);
                match result {
                    Ok(snapshot) => self.apply_refresh(snapshot),
                    Err(err) => {
                        self.backend_status_label
                            .set_text(&format!("Backend refresh failed: {err}"));
                    },
                }
            },
            UiMsg::ActionFinished { label, result } => {
                match result {
                    Ok(()) => {
                        self.set_validation_message("", false);
                        self.backend_status_label
                            .set_text(&format!("{label} succeeded"));
                    },
                    Err(err) => {
                        self.set_validation_message(&format!("{label} failed: {err}"), true);
                    },
                }
                self.request_refresh();
            },
        }
    }

    fn request_refresh(self: &Rc<Self>) {
        if self.refresh_in_flight.replace(true) {
            return;
        }
        let tx = self.sender.clone();
        thread::spawn(move || {
            let result = collect_refresh_snapshot().map_err(|e| e.to_string());
            let _ = tx.send(UiMsg::RefreshFinished(result));
        });
    }

    fn apply_refresh(self: &Rc<Self>, snapshot: RefreshSnapshot) {
        self.runtime_profiles.replace(snapshot.profiles.clone());
        self.backend_status_label
            .set_text(if snapshot.backend_available {
                "Backend: available"
            } else {
                "Backend: unavailable (editing still works)"
            });

        self.render_profile_list();
        self.refresh_runtime_section();
        self.update_action_sensitivity(snapshot.backend_available);
    }

    fn render_profile_list(self: &Rc<Self>) {
        while let Some(row) = self.profile_list.row_at_index(0) {
            self.profile_list.remove(&row);
        }

        let selected_id = self.selected_id.borrow().clone();
        let profiles = self.runtime_profiles.borrow().clone();

        if profiles.is_empty() {
            let row = gtk::ListBoxRow::new();
            row.set_selectable(false);
            row.set_activatable(false);
            let label = gtk::Label::new(Some("No profiles found"));
            label.set_margin_top(8);
            label.set_margin_bottom(8);
            label.set_margin_start(12);
            label.set_margin_end(12);
            label.set_xalign(0.0);
            row.set_child(Some(&label));
            self.profile_list.append(&row);
            return;
        }

        let mut row_to_select: Option<gtk::ListBoxRow> = None;
        for profile in profiles {
            let row = gtk::ListBoxRow::new();
            row.set_widget_name(&profile.id);

            let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 4);
            wrapper.set_margin_top(8);
            wrapper.set_margin_bottom(8);
            wrapper.set_margin_start(12);
            wrapper.set_margin_end(12);

            let title = gtk::Label::new(Some(&profile.name));
            title.set_xalign(0.0);
            title.add_css_class("heading");
            wrapper.append(&title);

            let mut sub = format!("{}  •  {}", profile.id, profile.status);
            if profile.autostart {
                sub.push_str("  •  autostart");
            }
            if profile.status == "failed" && !profile.last_error.is_empty() {
                sub.push_str("\n");
                sub.push_str(&profile.last_error);
            }
            let subtitle = gtk::Label::new(Some(&sub));
            subtitle.set_xalign(0.0);
            subtitle.set_wrap(true);
            subtitle.add_css_class("dim-label");
            wrapper.append(&subtitle);

            row.set_child(Some(&wrapper));
            self.profile_list.append(&row);

            if selected_id.as_deref() == Some(profile.id.as_str()) {
                row_to_select = Some(row);
            }
        }

        if let Some(row) = row_to_select {
            self.profile_list.select_row(Some(&row));
        }
    }

    fn update_action_sensitivity(&self, backend_available: bool) {
        let selected = self.current_editor_id_or_selected();
        let has_id = selected.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
        self.connect_button
            .set_sensitive(backend_available && has_id);
        self.disconnect_button
            .set_sensitive(backend_available && has_id);
    }

    fn refresh_runtime_section(&self) {
        let selected_id = self.current_editor_id_or_selected();
        let Some(id) = selected_id else {
            self.runtime_status_label.set_text("Status: disconnected");
            self.runtime_error_label.set_text("");
            return;
        };

        if let Some(profile) = self
            .runtime_profiles
            .borrow()
            .iter()
            .find(|p| p.id == id)
            .cloned()
        {
            self.runtime_status_label
                .set_text(&format!("Status: {}", profile.status));
            if profile.status == "failed" && !profile.last_error.is_empty() {
                self.runtime_error_label
                    .set_text(&format!("Last error: {}", profile.last_error));
            } else {
                self.runtime_error_label.set_text("");
            }
        } else {
            self.runtime_status_label.set_text("Status: unknown");
            self.runtime_error_label.set_text("");
        }
    }

    fn reset_editor_for_new_profile(self: &Rc<Self>) {
        self.selected_id.replace(None);

        self.form_title_label.set_text("New Profile");
        self.id_entry.set_text("");
        self.name_entry.set_text("");
        self.user_entry.set_text("");
        self.host_entry.set_text("");
        self.ssh_port_spin.set_value(22.0);
        self.identity_entry.set_text("");
        self.proxy_jump_entry.set_text("");
        self.autostart_switch.set_active(false);
        self.runtime_status_label.set_text("Status: disconnected");
        self.runtime_error_label.set_text("");
        self.clear_forward_rows();
        self.delete_button.set_sensitive(false);
        self.add_forward_row(Some(Forward::Local(LocalForward {
            bind_address: Some("127.0.0.1".into()),
            local_port: 8080,
            remote_host: "localhost".into(),
            remote_port: 8080,
        })));
        self.update_action_sensitivity(false);
    }

    fn load_profile_into_editor(self: &Rc<Self>, id: &str) {
        match profile::load_profile_by_id(id) {
            Ok(profile) => {
                self.clear_validation();
                self.selected_id.replace(Some(id.to_string()));
                self.populate_editor(id, &profile);
            },
            Err(err) => {
                self.set_validation_message(&format!("Failed to load profile {id}: {err}"), true);
            },
        }
    }

    fn populate_editor(self: &Rc<Self>, id: &str, profile: &Profile) {
        self.form_title_label
            .set_text(&format!("Edit Profile: {}", profile.name));
        self.id_entry.set_text(id);
        self.name_entry.set_text(&profile.name);
        self.user_entry.set_text(&profile.destination.user);
        self.host_entry.set_text(&profile.destination.host);
        self.ssh_port_spin
            .set_value(profile.destination.port as f64);
        self.identity_entry.set_text(
            &profile
                .identity_file
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        self.proxy_jump_entry
            .set_text(profile.proxy_jump.as_deref().unwrap_or(""));
        self.autostart_switch.set_active(profile.autostart);

        self.clear_forward_rows();
        for forward in &profile.forwards {
            self.add_forward_row(Some(forward.clone()));
        }
        if profile.forwards.is_empty() {
            self.add_forward_row(None);
        }

        self.delete_button.set_sensitive(true);
        self.refresh_runtime_section();
        let backend_available = self.backend_status_label.text().contains("available");
        self.update_action_sensitivity(backend_available);
    }

    fn clear_forward_rows(&self) {
        while let Some(row) = self.forwards_list.row_at_index(0) {
            self.forwards_list.remove(&row);
        }
        self.forward_rows.borrow_mut().clear();
    }

    fn add_forward_row(self: &Rc<Self>, initial: Option<Forward>) {
        let row_id = self.next_forward_row_id.get();
        self.next_forward_row_id.set(row_id + 1);

        let row = gtk::ListBoxRow::new();
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 8);
        outer.set_margin_top(8);
        outer.set_margin_bottom(8);
        outer.set_margin_start(12);
        outer.set_margin_end(12);

        let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        outer.append(&top);

        let kind = gtk::ComboBoxText::new();
        kind.append(Some("local"), "Local (-L)");
        kind.append(Some("remote"), "Remote (-R)");
        top.append(&kind);

        let remove_button = gtk::Button::with_label("Remove");
        remove_button.add_css_class("flat");
        top.append(&remove_button);

        let grid = gtk::Grid::new();
        grid.set_column_spacing(8);
        grid.set_row_spacing(8);
        outer.append(&grid);

        let bind_entry = gtk::Entry::new();
        bind_entry.set_placeholder_text(Some("optional bind address"));
        let port1_label = gtk::Label::new(Some("Local Port"));
        port1_label.set_xalign(0.0);
        let port1_spin = gtk::SpinButton::with_range(1.0, 65535.0, 1.0);
        let host_label = gtk::Label::new(Some("Remote Host"));
        host_label.set_xalign(0.0);
        let host_entry = gtk::Entry::new();
        let port2_label = gtk::Label::new(Some("Remote Port"));
        port2_label.set_xalign(0.0);
        let port2_spin = gtk::SpinButton::with_range(1.0, 65535.0, 1.0);

        grid.attach(&gtk::Label::new(Some("Bind Address")), 0, 0, 1, 1);
        grid.attach(&bind_entry, 1, 0, 1, 1);
        grid.attach(&port1_label, 0, 1, 1, 1);
        grid.attach(&port1_spin, 1, 1, 1, 1);
        grid.attach(&host_label, 0, 2, 1, 1);
        grid.attach(&host_entry, 1, 2, 1, 1);
        grid.attach(&port2_label, 0, 3, 1, 1);
        grid.attach(&port2_spin, 1, 3, 1, 1);

        row.set_child(Some(&outer));
        self.forwards_list.append(&row);

        let row_widgets = ForwardRowWidgets {
            row_id,
            row: row.clone(),
            kind: kind.clone(),
            bind_entry: bind_entry.clone(),
            port1_label: port1_label.clone(),
            port1_spin: port1_spin.clone(),
            host_label: host_label.clone(),
            host_entry: host_entry.clone(),
            port2_label: port2_label.clone(),
            port2_spin: port2_spin.clone(),
        };
        self.forward_rows.borrow_mut().push(row_widgets);

        let weak = Rc::downgrade(self);
        kind.connect_changed(move |combo| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let selected = combo.active_id().map(|s| s.to_string()).unwrap_or_default();
            ui.update_forward_row_labels(row_id, &selected);
        });

        let weak = Rc::downgrade(self);
        remove_button.connect_clicked(move |_| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            ui.remove_forward_row(row_id);
        });

        match initial {
            Some(Forward::Local(fwd)) => {
                kind.set_active_id(Some("local"));
                bind_entry.set_text(fwd.bind_address.as_deref().unwrap_or(""));
                port1_spin.set_value(fwd.local_port as f64);
                host_entry.set_text(&fwd.remote_host);
                port2_spin.set_value(fwd.remote_port as f64);
            },
            Some(Forward::Remote(fwd)) => {
                kind.set_active_id(Some("remote"));
                bind_entry.set_text(fwd.bind_address.as_deref().unwrap_or(""));
                port1_spin.set_value(fwd.remote_port as f64);
                host_entry.set_text(&fwd.local_host);
                port2_spin.set_value(fwd.local_port as f64);
            },
            None => {
                kind.set_active_id(Some("local"));
                bind_entry.set_text("127.0.0.1");
                port1_spin.set_value(8080.0);
                host_entry.set_text("localhost");
                port2_spin.set_value(8080.0);
            },
        }
        self.update_forward_row_labels(row_id, &kind.active_id().unwrap().to_string());
    }

    fn update_forward_row_labels(&self, row_id: u64, kind: &str) {
        let rows = self.forward_rows.borrow();
        let Some(row) = rows.iter().find(|r| r.row_id == row_id) else {
            return;
        };
        if kind == "remote" {
            row.port1_label.set_text("Remote Port");
            row.host_label.set_text("Local Host");
            row.port2_label.set_text("Local Port");
        } else {
            row.port1_label.set_text("Local Port");
            row.host_label.set_text("Remote Host");
            row.port2_label.set_text("Remote Port");
        }
    }

    fn remove_forward_row(&self, row_id: u64) {
        let mut rows = self.forward_rows.borrow_mut();
        if let Some(idx) = rows.iter().position(|r| r.row_id == row_id) {
            let row = rows.remove(idx).row;
            self.forwards_list.remove(&row);
        }
    }

    fn current_editor_id_or_selected(&self) -> Option<String> {
        let id = self.id_entry.text().to_string();
        if !id.trim().is_empty() {
            return Some(id.trim().to_string());
        }
        self.selected_id.borrow().clone()
    }

    fn save_current_profile(self: &Rc<Self>) {
        self.clear_validation();

        match self.build_profile_from_form() {
            Ok((id, profile)) => {
                if let Err(err) = self.ensure_unique_name(&id, &profile.name) {
                    self.set_validation_message(&err.to_string(), true);
                    return;
                }

                let save_result = (|| -> Result<()> {
                    profile::save_profile_by_id(&id, &profile)?;
                    sync_autostart(&id, profile.autostart)?;
                    Ok(())
                })();

                match save_result {
                    Ok(()) => {
                        self.selected_id.replace(Some(id.clone()));
                        self.form_title_label
                            .set_text(&format!("Edit Profile: {}", profile.name));
                        self.delete_button.set_sensitive(true);
                        self.backend_status_label.set_text("Profile saved");
                        self.request_refresh();
                    },
                    Err(err) => {
                        self.set_validation_message(&format!("Save failed: {err}"), true);
                    },
                }
            },
            Err(err) => self.set_validation_message(&err, true),
        }
    }

    fn ensure_unique_name(&self, current_id: &str, name: &str) -> Result<()> {
        let target = name.trim();
        for entry in profile::list_profiles()? {
            if entry.id != current_id && entry.profile.name.trim() == target {
                return Err(anyhow!(
                    "profile name '{}' is already used by id '{}'",
                    target,
                    entry.id
                ));
            }
        }
        Ok(())
    }

    fn delete_selected_profile(self: &Rc<Self>) {
        let Some(id) = self.current_editor_id_or_selected() else {
            self.set_validation_message("No profile selected", true);
            return;
        };

        let result = (|| -> Result<()> {
            let path = profile::profile_path_for_id(&id)?;
            if path.exists() {
                std::fs::remove_file(&path)
                    .with_context(|| format!("removing {}", path.display()))?;
            }
            let _ = sync_autostart(&id, false);
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.backend_status_label
                    .set_text(&format!("Deleted profile '{id}'"));
                self.reset_editor_for_new_profile();
                self.request_refresh();
            },
            Err(err) => self.set_validation_message(&format!("Delete failed: {err}"), true),
        }
    }

    fn build_profile_from_form(&self) -> std::result::Result<(String, Profile), String> {
        let mut id = self.id_entry.text().trim().to_string();
        let name = self.name_entry.text().trim().to_string();
        if id.is_empty() && !name.is_empty() {
            id = slugify_profile_id(&name);
            self.id_entry.set_text(&id);
        }

        if id.is_empty() {
            return Err("Profile ID is required".into());
        }

        let identity_file = trim_optional(&self.identity_entry);
        let proxy_jump = trim_optional(&self.proxy_jump_entry);

        let mut forwards = Vec::new();
        for row in self.forward_rows.borrow().iter() {
            let kind = row
                .kind
                .active_id()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let bind_address = {
                let text = row.bind_entry.text().trim().to_string();
                if text.is_empty() { None } else { Some(text) }
            };
            let port1 = row.port1_spin.value_as_int() as u16;
            let host = row.host_entry.text().trim().to_string();
            let port2 = row.port2_spin.value_as_int() as u16;

            let forward = if kind == "remote" {
                Forward::Remote(RemoteForward {
                    bind_address,
                    remote_port: port1,
                    local_host: host,
                    local_port: port2,
                })
            } else {
                Forward::Local(LocalForward {
                    bind_address,
                    local_port: port1,
                    remote_host: host,
                    remote_port: port2,
                })
            };
            forwards.push(forward);
        }

        let profile = Profile {
            schema: SCHEMA_VERSION,
            name,
            autostart: self.autostart_switch.is_active(),
            destination: Destination {
                user: self.user_entry.text().trim().to_string(),
                host: self.host_entry.text().trim().to_string(),
                port: self.ssh_port_spin.value_as_int() as u16,
            },
            identity_file: identity_file.map(PathBuf::from),
            proxy_jump,
            forwards,
        };

        profile.validate().map_err(|e| e.to_string())?;
        Ok((id, profile))
    }

    fn invoke_backend_action(
        self: &Rc<Self>,
        method: &'static str,
        label: &str,
        id: Option<String>,
    ) {
        let Some(id) = id else {
            self.set_validation_message("Select or save a profile first", true);
            return;
        };
        let tx = self.sender.clone();
        let label_str = format!("{label} {id}");
        thread::spawn(move || {
            let result = call_backend_void(method, &id).map_err(|e| e.to_string());
            let _ = tx.send(UiMsg::ActionFinished {
                label: label_str,
                result,
            });
        });
    }

    fn set_validation_message(&self, message: &str, is_error: bool) {
        self.validation_label.set_text(message);
        if is_error {
            self.validation_label.add_css_class("error");
        } else {
            self.validation_label.remove_css_class("error");
        }
    }

    fn clear_validation(&self) {
        self.set_validation_message("", false);
    }
}

fn attach_labeled<W: IsA<gtk::Widget>>(grid: &gtk::Grid, row: i32, label_text: &str, widget: &W) {
    let label = gtk::Label::new(Some(label_text));
    label.set_xalign(0.0);
    grid.attach(&label, 0, row, 1, 1);
    grid.attach(widget, 1, row, 1, 1);
}

fn trim_optional(entry: &gtk::Entry) -> Option<String> {
    let text = entry.text().trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn slugify_profile_id(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "profile".to_string()
    } else {
        out
    }
}

fn collect_refresh_snapshot() -> Result<RefreshSnapshot> {
    let mut by_id: HashMap<String, RuntimeProfile> = HashMap::new();
    for entry in profile::list_profiles()? {
        by_id.insert(
            entry.id.clone(),
            RuntimeProfile {
                id: entry.id,
                name: entry.profile.name,
                status: "disconnected".into(),
                autostart: entry.profile.autostart,
                last_error: String::new(),
            },
        );
    }

    let mut backend_available = false;
    if let Ok(rows) = list_profiles_via_backend() {
        backend_available = true;
        for (id, name, status, autostart) in rows {
            let entry = by_id.entry(id.clone()).or_insert(RuntimeProfile {
                id: id.clone(),
                name: name.clone(),
                status: status.clone(),
                autostart,
                last_error: String::new(),
            });
            entry.name = name;
            entry.status = status.clone();
            entry.autostart = autostart;
        }
    }

    let mut profiles = by_id.into_values().collect::<Vec<_>>();
    profiles.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });

    for profile in &mut profiles {
        if profile.status == "failed" {
            profile.last_error = last_journal_line_for_profile(&profile.id).unwrap_or_default();
        }
    }

    Ok(RefreshSnapshot {
        backend_available,
        profiles,
    })
}

fn list_profiles_via_backend() -> Result<Vec<(String, String, String, bool)>> {
    let conn = zbus::blocking::Connection::session().context("connecting to session bus")?;
    let proxy = zbus::blocking::Proxy::new(&conn, BUS_NAME, OBJECT_PATH, IFACE_NAME)
        .context("creating backend D-Bus proxy")?;
    let rows: Vec<(String, String, String, bool)> = proxy
        .call("ListProfiles", &())
        .context("calling ListProfiles")?;
    Ok(rows)
}

fn call_backend_void(method: &str, id: &str) -> Result<()> {
    let conn = zbus::blocking::Connection::session().context("connecting to session bus")?;
    let proxy = zbus::blocking::Proxy::new(&conn, BUS_NAME, OBJECT_PATH, IFACE_NAME)
        .context("creating backend D-Bus proxy")?;
    let _: () = proxy
        .call(method, &(id,))
        .with_context(|| format!("calling {method}"))?;
    Ok(())
}

fn sync_autostart(id: &str, autostart: bool) -> Result<()> {
    let unit = profile::unit_name_for_id(id)?;
    if autostart {
        run_command("systemctl", &["--user", "enable", "--now", &unit])
            .with_context(|| format!("enabling autostart for {id}"))?;
    } else {
        run_command("systemctl", &["--user", "disable", "--now", &unit])
            .with_context(|| format!("disabling autostart for {id}"))?;
    }
    Ok(())
}

fn last_journal_line_for_profile(id: &str) -> Result<String> {
    let unit = profile::unit_name_for_id(id)?;
    let output = Command::new("journalctl")
        .args(["--user", "-u", &unit, "-n", "1", "--no-pager", "-o", "cat"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("running journalctl for {unit}"))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_command(program: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("running {program} {}", args.join(" ")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        String::new()
    };

    if detail.is_empty() {
        Err(anyhow!(
            "{program} {} exited with status {}",
            args.join(" "),
            output.status
        ))
    } else {
        Err(anyhow!(
            "{program} {} exited with status {}: {detail}",
            args.join(" "),
            output.status
        ))
    }
}
