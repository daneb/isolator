use crate::{audit, manifest, manifest::Manifest, paths, proc};
use anyhow::{Context, Result};
use std::io::{self, Write};

pub fn run(name: &str, image: &str, github: bool) -> Result<()> {
    manifest::validate_name(name)?;

    let dir = paths::project_dir(name)?;
    if dir.exists() {
        anyhow::bail!("project '{name}' already exists at {}", dir.display());
    }

    let mut m = Manifest::new(name, image);

    if github {
        print!(
            "About to run `gh repo create {name} --private` on your GitHub account. Continue? [y/N] "
        );
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("Aborted — no GitHub repo created, no project scaffolded.");
            return Ok(());
        }

        let (status, whoami) = proc::run_capture("gh", &["api", "user", "-q", ".login"])?;
        proc::require_success("gh api user", status)?;
        let owner = whoami.trim();
        let repo_slug = format!("{owner}/{name}");

        let status = proc::run_inherit("gh", &["repo", "create", &repo_slug, "--private"])?;
        proc::require_success("gh repo create", status)?;
        m.github_repo = Some(repo_slug);
    }

    paths::ensure_project_dirs(name)?;
    m.save(&paths::manifest_path(name)?)
        .context("writing isolator.yaml")?;

    println!("==> starting sandbox + egress for '{name}'");
    super::compose_up(name)?;

    if let Some(repo) = &m.github_repo {
        let clone_url = format!("https://github.com/{repo}.git");
        println!("==> cloning {clone_url} into the sandbox workspace volume");
        let status = proc::run_inherit(
            "docker",
            &[
                "exec",
                &m.sandbox_container(),
                "git",
                "clone",
                &clone_url,
                ".",
            ],
        )?;
        audit::log_exec(name, &["git".into(), "clone".into(), clone_url], status.code())?;
        proc::require_success("git clone in sandbox", status)?;
    }

    println!("==> running `keel init` inside the sandbox");
    let status = proc::run_inherit("docker", &["exec", &m.sandbox_container(), "keel", "init"]);
    match status {
        Ok(s) => {
            audit::log_exec(name, &["keel".into(), "init".into()], s.code())?;
            if !s.success() {
                println!(
                    "note: `keel init` did not exit cleanly — check with `isolator shell {name}`"
                );
            }
        }
        Err(e) => println!("note: could not run `keel init` automatically: {e}"),
    }

    println!(
        "\n'{name}' is up. Manifest: {}\nNext: isolator shell {name}",
        paths::manifest_path(name)?.display()
    );
    Ok(())
}
