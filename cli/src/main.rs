mod audit;
mod commands;
mod compose;
mod manifest;
mod paths;
mod proc;

use clap::{Parser, Subcommand};

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
    /// Show the host-side audit trail for a project.
    Audit { name: String },
    /// Verify a running project's container actually has every hardening
    /// control applied (read-only rootfs, dropped capabilities, no bind
    /// mounts, non-root user, no default-bridge network, ...).
    Selftest { name: String },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::New { name, image, github } => commands::new_cmd::run(&name, &image, github),
        Command::Up { name } => commands::up::run(&name),
        Command::Down { name } => commands::down::run(&name),
        Command::Shell { name } => commands::shell::run(&name),
        Command::Run { name, cmd } => commands::run_cmd::run(&name, &cmd),
        Command::Status => commands::status::run(),
        Command::Audit { name } => commands::audit_cmd::run(&name),
        Command::Selftest { name } => commands::selftest::run(&name),
    };

    if let Err(e) = result {
        eprintln!("error: {e:?}");
        std::process::exit(1);
    }
}
