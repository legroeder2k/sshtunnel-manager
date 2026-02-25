use std::collections::HashMap;
use std::process::{Command, Stdio};

use anyhow::{Context as AnyhowContext, Result, anyhow};
use profile::ProfileEntry;
use zbus::fdo;
use zbus::{SignalContext, interface};

const BUS_NAME: &str = "com.legroeder2k.SshTunnelManager";
const OBJECT_PATH: &str = "/com/legroeder2k/SshTunnelManager";
const IFACE_NAME: &str = "com.legroeder2k.SshTunnelManager1";

#[derive(Debug, Clone)]
struct BackendService;

#[derive(Debug, Clone, Default)]
struct UnitProps {
    active_state: String,
    sub_state: String,
    result: String,
    exec_main_status: Option<i32>,
    exec_main_code: Option<i32>,
}

#[derive(Debug, Clone)]
struct TunnelStatus {
    status: String,
    message: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let service = BackendService;
    let _conn = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, service)?
        .build()
        .await?;

    eprintln!("sshtunnel-manager-backendd: serving {IFACE_NAME} at {OBJECT_PATH} on {BUS_NAME}");

    tokio::signal::ctrl_c().await?;
    Ok(())
}

#[interface(name = "com.legroeder2k.SshTunnelManager1")]
impl BackendService {
    async fn list_profiles(&self) -> fdo::Result<Vec<(String, String, String, bool)>> {
        let entries = profile::list_profiles().map_err(to_fdo)?;
        let mut out = Vec::with_capacity(entries.len());
        for entry in entries {
            let status = status_for_id(&entry.id).unwrap_or_else(|_| TunnelStatus {
                status: "disconnected".to_string(),
                message: String::new(),
            });
            out.push((
                entry.id,
                entry.profile.name,
                status.status,
                entry.profile.autostart,
            ));
        }
        Ok(out)
    }

    async fn get_profile(&self, id: &str) -> fdo::Result<String> {
        let path = profile::profile_path_for_id(id).map_err(to_fdo)?;
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| fdo::Error::Failed(format!("reading {}: {e}", path.display())))?;
        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| fdo::Error::Failed(format!("parsing {}: {e}", path.display())))?;
        serde_json::to_string_pretty(&parsed).map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    async fn connect(
        &self,
        id: &str,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<()> {
        profile::validate_profile_id(id).map_err(to_fdo)?;
        let _ = profile::load_profile_by_id(id).map_err(to_fdo)?;
        let unit = profile::unit_name_for_id(id).map_err(to_fdo)?;
        run_cmd("systemctl", &["--user", "start", &unit]).map_err(to_fdo)?;
        self.emit_status_for(id, &ctxt).await?;
        Ok(())
    }

    async fn disconnect(
        &self,
        id: &str,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<()> {
        profile::validate_profile_id(id).map_err(to_fdo)?;
        let unit = profile::unit_name_for_id(id).map_err(to_fdo)?;
        run_cmd("systemctl", &["--user", "stop", &unit]).map_err(to_fdo)?;
        self.emit_status_for(id, &ctxt).await?;
        Ok(())
    }

    async fn connect_all(
        &self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<()> {
        let entries = profile::list_profiles().map_err(to_fdo)?;
        for ProfileEntry { id, .. } in entries {
            let unit = profile::unit_name_for_id(&id).map_err(to_fdo)?;
            if let Err(err) = run_cmd("systemctl", &["--user", "start", &unit]) {
                let status = status_for_id(&id).unwrap_or_else(|_| TunnelStatus {
                    status: "failed".to_string(),
                    message: err.to_string(),
                });
                Self::profile_status_changed(&ctxt, &id, &status.status, &status.message)
                    .await
                    .map_err(|e| fdo::Error::Failed(e.to_string()))?;
                return Err(to_fdo(err));
            }
            self.emit_status_for(&id, &ctxt).await?;
        }
        Ok(())
    }

    async fn disconnect_all(
        &self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<()> {
        let entries = profile::list_profiles().map_err(to_fdo)?;
        for ProfileEntry { id, .. } in entries {
            let unit = profile::unit_name_for_id(&id).map_err(to_fdo)?;
            if let Err(err) = run_cmd("systemctl", &["--user", "stop", &unit]) {
                let status = status_for_id(&id).unwrap_or_else(|_| TunnelStatus {
                    status: "failed".to_string(),
                    message: err.to_string(),
                });
                Self::profile_status_changed(&ctxt, &id, &status.status, &status.message)
                    .await
                    .map_err(|e| fdo::Error::Failed(e.to_string()))?;
                return Err(to_fdo(err));
            }
            self.emit_status_for(&id, &ctxt).await?;
        }
        Ok(())
    }

    async fn get_status(&self, id: &str) -> fdo::Result<String> {
        profile::validate_profile_id(id).map_err(to_fdo)?;
        Ok(status_for_id(id).map_err(to_fdo)?.status)
    }

    #[zbus(signal)]
    async fn profile_status_changed(
        ctxt: &SignalContext<'_>,
        id: &str,
        status: &str,
        message: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn profiles_changed(ctxt: &SignalContext<'_>) -> zbus::Result<()>;
}

impl BackendService {
    async fn emit_status_for(&self, id: &str, ctxt: &SignalContext<'_>) -> fdo::Result<()> {
        let status = status_for_id(id).map_err(to_fdo)?;
        Self::profile_status_changed(ctxt, id, &status.status, &status.message)
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))
    }
}

fn to_fdo(err: anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(err.to_string())
}

fn run_cmd(program: &str, args: &[&str]) -> Result<()> {
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

fn status_for_id(id: &str) -> Result<TunnelStatus> {
    profile::validate_profile_id(id)?;
    let unit = profile::unit_name_for_id(id)?;
    let props = systemd_show(&unit)?;
    let mut status = map_systemd_status(&props);
    if status.status == "failed" {
        let journal_line = last_journal_line(&unit).unwrap_or_default();
        status.message = friendly_failure_message(&status.message, &journal_line);
    }
    Ok(status)
}

fn systemd_show(unit: &str) -> Result<UnitProps> {
    let output = Command::new("systemctl")
        .args([
            "--user",
            "show",
            unit,
            "--property=ActiveState",
            "--property=SubState",
            "--property=Result",
            "--property=ExecMainStatus",
            "--property=ExecMainCode",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("running systemctl --user show {unit}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "systemctl --user show {unit} failed ({}): {}",
            output.status,
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut map = HashMap::<String, String>::new();
    for line in stdout.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    Ok(UnitProps {
        active_state: map.remove("ActiveState").unwrap_or_default(),
        sub_state: map.remove("SubState").unwrap_or_default(),
        result: map.remove("Result").unwrap_or_default(),
        exec_main_status: map
            .remove("ExecMainStatus")
            .and_then(|v| v.parse::<i32>().ok()),
        exec_main_code: map
            .remove("ExecMainCode")
            .and_then(|v| v.parse::<i32>().ok()),
    })
}

fn map_systemd_status(props: &UnitProps) -> TunnelStatus {
    let status = match props.active_state.as_str() {
        "active" => "connected",
        "activating" => "connecting",
        "failed" => "failed",
        _ => "disconnected",
    }
    .to_string();

    let mut message = String::new();
    if status == "failed" {
        if let Some(code) = props.exec_main_status {
            message = if let Some(exec_code) = props.exec_main_code {
                format!(
                    "systemd result={} exec_code={} exit_status={}",
                    blank_as_default(&props.result, "failed"),
                    exec_code,
                    code
                )
            } else {
                format!(
                    "systemd result={} exit_status={}",
                    blank_as_default(&props.result, "failed"),
                    code
                )
            };
        } else if !props.result.is_empty() {
            message = format!("systemd result={}", props.result);
        } else if !props.sub_state.is_empty() {
            message = format!("systemd substate={}", props.sub_state);
        }
    }

    TunnelStatus { status, message }
}

fn blank_as_default<'a>(value: &'a str, default: &'a str) -> &'a str {
    if value.is_empty() { default } else { value }
}

fn friendly_failure_message(systemd_message: &str, journal_line: &str) -> String {
    if let Some(msg) = host_key_failure_message(journal_line) {
        return msg;
    }

    if !journal_line.trim().is_empty()
        && (systemd_message.trim().is_empty() || systemd_message.contains("systemd result="))
    {
        return journal_line.trim().to_string();
    }

    if !systemd_message.trim().is_empty() {
        return systemd_message.trim().to_string();
    }

    journal_line.trim().to_string()
}

fn host_key_failure_message(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if lower.contains("host key verification failed")
        || lower.contains("authenticity of host")
        || lower.contains("are you sure you want to continue connecting")
    {
        return Some(
            "Host key not trusted yet. Connect once in a terminal or set Host Key Policy to 'Accept New'."
                .to_string(),
        );
    }

    if lower.contains("remote host identification has changed") {
        return Some(
            "Host key changed. Verify the server and remove/update the old known_hosts entry before reconnecting."
                .to_string(),
        );
    }

    None
}

fn last_journal_line(unit: &str) -> Result<String> {
    let output = Command::new("journalctl")
        .args(["--user", "-u", unit, "-n", "1", "--no-pager", "-o", "cat"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("running journalctl --user -u {unit}"))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_active_to_connected() {
        let status = map_systemd_status(&UnitProps {
            active_state: "active".into(),
            sub_state: "running".into(),
            ..UnitProps::default()
        });
        assert_eq!(status.status, "connected");
        assert!(status.message.is_empty());
    }

    #[test]
    fn maps_failed_with_message() {
        let status = map_systemd_status(&UnitProps {
            active_state: "failed".into(),
            result: "exit-code".into(),
            exec_main_code: Some(1),
            exec_main_status: Some(255),
            ..UnitProps::default()
        });
        assert_eq!(status.status, "failed");
        assert!(status.message.contains("exit_status=255"));
    }

    #[test]
    fn rewrites_unknown_host_key_failure_to_actionable_message() {
        let msg = host_key_failure_message("Host key verification failed.").expect("message");
        assert!(msg.contains("Host key not trusted yet"));
        assert!(msg.contains("Accept New"));
    }

    #[test]
    fn prefers_journal_line_over_generic_systemd_failure() {
        let msg = friendly_failure_message(
            "systemd result=exit-code exit_status=255",
            "Connection refused",
        );
        assert_eq!(msg, "Connection refused");
    }
}
