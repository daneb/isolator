mod audit;
mod canary;
mod commands;
mod compose;
mod egress_log;
mod manifest;
mod paths;
mod proc;
mod secrets;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "isolator", about = "Containerized, keel-driven AI sandboxes")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new project: manifest, sandbox+egress containers, optional
    /// GitHub repo, `keel init` inside the sandbox.
    New {
        name: String,
        /// isolator/base, isolator/node, isolator/rust, or isolator/python
        #[arg(long, default_value = "isolator/base:latest")]
        image: String,
        /// Also create a private GitHub repo via `gh repo create` (asks
        /// for confirmation before doing anything).
        #[arg(long)]
        github: bool,
    },
    /// Bring an existing local git repo (e.g. one you're already working
    /// on outside isolator) into a new sandboxed project. Transfers its
    /// full history via a `git bundle` — the source directory is never
    /// bind-mounted, only a one-shot bundle file crosses into the
    /// container. Preserves an existing GitHub remote if there is one.
    Import {
        name: String,
        /// Path to the existing local repo to import.
        #[arg(long, value_name = "PATH")]
        from: PathBuf,
        /// isolator/base, isolator/node, isolator/rust, or isolator/python
        /// — auto-detected from the source repo (Cargo.toml, package.json,
        /// pyproject.toml/requirements.txt) if not given.
        #[arg(long)]
        image: Option<String>,
        /// If the source repo has no GitHub remote, create one (asks for
        /// confirmation). Ignored if it already has one.
        #[arg(long)]
        github: bool,
    },
    /// Start (or restart) a project's sandbox + egress containers.
    Up { name: String },
    /// Stop a project's containers.
    Down { name: String },
    /// Open an interactive shell inside a project's sandbox.
    Shell { name: String },
    /// Run one command inside a project's sandbox (e.g. `keel run <spec>`).
    Run {
        name: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        cmd: Vec<String>,
    },
    /// List all projects and their container status.
    Status,
    /// Show the host-side audit trail for a project (folds in new egress
    /// gateway log entries first).
    Audit {
        name: String,
        /// Recompute the hash chain and report whether it's intact instead
        /// of printing the trail.
        #[arg(long)]
        verify: bool,
        /// Export the chain plus the latest keel evidence bundle as a
        /// tar.gz into this directory instead of printing the trail.
        #[arg(long, value_name = "DIR")]
        export: Option<PathBuf>,
    },
    /// Verify a running project's container actually has every hardening
    /// control applied (read-only rootfs, dropped capabilities, no bind
    /// mounts, non-root user, no default-bridge network, ...) and run the
    /// active breakout battery (canary domain, read-only fs, docker.sock).
    Selftest { name: String },
    /// Manage a project's secrets in the macOS Keychain, as an
    /// alternative to exporting them into your shell before every
    /// `isolator up`/`new`.
    #[command(subcommand)]
    Secrets(SecretsCommand),
}

#[derive(Subcommand)]
enum SecretsCommand {
    /// Store a secret's value in the Keychain (prompts for it, hidden
    /// where the terminal supports it). Overwrites any existing value.
    Set { project: String, name: String },
    /// Remove a secret from the Keychain.
    Unset { project: String, name: String },
    /// Show where each of the project's declared secrets (isolator.yaml's
    /// `secrets:` list) would currently be resolved from: your shell's
    /// environment, the Keychain, or neither.
    Status { project: String },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::New { name, image, github } => commands::new_cmd::run(&name, &image, github),
        Command::Import { name, from, image, github } => commands::import_cmd::run(&name, &from, image, github),
        Command::Up { name } => commands::up::run(&name),
        Command::Down { name } => commands::down::run(&name),
        Command::Shell { name } => commands::shell::run(&name),
        Command::Run { name, cmd } => commands::run_cmd::run(&name, &cmd),
        Command::Status => commands::status::run(),
        Command::Audit { name, verify, export } => commands::audit_cmd::run(&name, verify, export),
        Command::Selftest { name } => commands::selftest::run(&name),
        Command::Secrets(SecretsCommand::Set { project, name }) => secrets::set(&project, &name),
        Command::Secrets(SecretsCommand::Unset { project, name }) => secrets::unset(&project, &name),
        Command::Secrets(SecretsCommand::Status { project }) => commands::secrets_status::run(&project),
    };

    if let Err(e) = result {
        eprintln!("error: {e:?}");
        std::process::exit(1);
    }
}
