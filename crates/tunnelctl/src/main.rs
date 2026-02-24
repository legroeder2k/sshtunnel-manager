use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "tunnelctl")]
#[command(about = "CLI for managing SSH tunnel systemd user services")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// List configured tunnel profiles and current unit state.
    List,
    /// Start a tunnel service for a profile id.
    Up { id: String },
    /// Stop a tunnel service for a profile id.
    Down { id: String },
    /// Show systemd status for a profile id.
    Status { id: String },
    /// Show journal logs for a profile id.
    Logs {
        id: String,
        /// Number of recent log lines to show.
        #[arg(short = 'n', long, default_value_t = 100)]
        lines: usize,
        /// Follow the logs stream.
        #[arg(short = 'f', long)]
        follow: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::List => cmd_list(),
        Commands::Up { id } => cmd_up(&id),
        Commands::Down { id } => cmd_down(&id),
        Commands::Status { id } => cmd_status(&id),
        Commands::Logs { id, lines, follow } => cmd_logs(&id, lines, follow),
    }
}

fn cmd_list() -> Result<()> {
    let entries = profile::list_profiles()?;
    if entries.is_empty() {
        println!(
            "No profiles found in {}",
            profile::profiles_dir()?.display()
        );
        return Ok(());
    }

    println!("{:<24} {:<12} {:<5} NAME", "ID", "STATUS", "AUTO");
    for entry in entries {
        let status = unit_is_active(&entry.id).unwrap_or_else(|_| "unknown".to_string());
        let autostart = if entry.profile.autostart { "yes" } else { "no" };
        println!(
            "{:<24} {:<12} {:<5} {}",
            entry.id, status, autostart, entry.profile.name
        );
    }
    Ok(())
}

fn cmd_up(id: &str) -> Result<()> {
    profile::validate_profile_id(id)?;
    let _ = profile::load_profile_by_id(id)?;
    run_and_check(
        "systemctl",
        &["--user", "start", &profile::unit_name_for_id(id)?],
        true,
    )
}

fn cmd_down(id: &str) -> Result<()> {
    profile::validate_profile_id(id)?;
    run_and_check(
        "systemctl",
        &["--user", "stop", &profile::unit_name_for_id(id)?],
        true,
    )
}

fn cmd_status(id: &str) -> Result<()> {
    profile::validate_profile_id(id)?;
    run_and_check(
        "systemctl",
        &[
            "--user",
            "status",
            "--no-pager",
            &profile::unit_name_for_id(id)?,
        ],
        true,
    )
}

fn cmd_logs(id: &str, lines: usize, follow: bool) -> Result<()> {
    profile::validate_profile_id(id)?;
    let unit = profile::unit_name_for_id(id)?;
    let lines_str = lines.to_string();
    let mut args = vec![
        "--user",
        "-u",
        unit.as_str(),
        "-n",
        lines_str.as_str(),
        "--no-pager",
    ];
    if follow {
        args.push("-f");
    }
    run_and_check("journalctl", &args, true)
}

fn unit_is_active(id: &str) -> Result<String> {
    let unit = profile::unit_name_for_id(id)?;
    let output = Command::new("systemctl")
        .args(["--user", "is-active", unit.as_str()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("running systemctl --user is-active")?;

    if !output.status.success() && output.stdout.is_empty() {
        return Ok("inactive".to_string());
    }

    let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if status.is_empty() {
        Ok("unknown".to_string())
    } else {
        Ok(status)
    }
}

fn run_and_check(program: &str, args: &[&str], inherit_stdio: bool) -> Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if inherit_stdio {
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    }

    let status = cmd
        .status()
        .with_context(|| format!("running {program} {}", args.join(" ")))?;
    if status.success() {
        Ok(())
    } else {
        bail!("{program} exited with status {status}")
    }
}
