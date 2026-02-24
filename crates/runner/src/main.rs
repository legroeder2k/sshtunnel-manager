use std::process::Command;

use anyhow::Result;
use clap::Parser;
use profile::Profile;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[derive(Debug, Parser)]
#[command(name = "sshtunnel-runner")]
#[command(about = "Load an SSH tunnel profile and exec ssh")]
struct Cli {
    /// Profile id (maps to ~/.config/sshtunnel-manager/profiles.d/<id>.json)
    profile_id: Option<String>,

    /// Read a profile from an explicit JSON path instead of a profile id.
    #[arg(long, conflicts_with = "profile_id")]
    profile_path: Option<std::path::PathBuf>,

    /// Print the ssh arguments and exit (useful for debugging).
    #[arg(long)]
    print_argv: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let profile = load_profile(&cli)?;
    let args = profile.ssh_argv()?;

    if cli.print_argv {
        for arg in &args {
            println!("{}", arg.to_string_lossy());
        }
        return Ok(());
    }

    exec_ssh(args)
}

fn load_profile(cli: &Cli) -> Result<Profile> {
    match (&cli.profile_id, &cli.profile_path) {
        (Some(id), None) => profile::load_profile_by_id(id),
        (None, Some(path)) => profile::load_profile_from_path(path),
        (Some(_), Some(_)) => unreachable!("clap enforces exclusivity"),
        (None, None) => Err(anyhow::anyhow!(
            "missing profile id (or use --profile-path <path>)"
        )),
    }
}

#[cfg(unix)]
fn exec_ssh(args: Vec<std::ffi::OsString>) -> Result<()> {
    let err = Command::new("ssh").args(&args).exec();
    Err(anyhow::Error::new(err).context("exec ssh"))
}

#[cfg(not(unix))]
fn exec_ssh(args: Vec<std::ffi::OsString>) -> Result<()> {
    let status = Command::new("ssh")
        .args(&args)
        .status()
        .context("spawn ssh")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("ssh exited with status {status}"))
    }
}
