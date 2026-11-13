mod config;
mod git;
mod guard;
mod profile;
mod tui;
mod ui;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use profile::Profile;
use ui::{bold, dim, green};

#[derive(Parser)]
#[command(
    name = "gas",
    version,
    about = "Git account switcher with push/commit leak guard"
)]
struct Cli {
    /// Run with no command to open the interactive picker
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Add or update a profile
    Add {
        /// Short profile id, e.g. work
        id: String,
        /// git user.name
        #[arg(long)]
        name: String,
        /// git user.email (use the GitHub noreply address to stay private)
        #[arg(long)]
        email: String,
        /// GitHub username (used for `gh auth switch`)
        #[arg(long)]
        github: Option<String>,
        /// SSH private key used for git@github.com remotes
        #[arg(long)]
        ssh_key: Option<String>,
    },
    /// Remove a profile
    Rm { id: String },
    /// List profiles
    Ls,
    /// Switch to a profile (this repo if inside one, else global)
    Use {
        id: String,
        /// Apply globally even inside a repo
        #[arg(short, long)]
        global: bool,
    },
    /// Show active identity, profile and remotes
    Status,
    /// Install the global pre-push and pre-commit guards
    Init,
    /// Remove the global guards
    Uninstall,
    /// Show the config file path and effective settings
    Config,
    /// Run by git hooks; not for manual use
    #[command(hide = true)]
    Guard {
        hook: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> Result<()> {
    config::migrate_legacy();
    // a broken config must never block git, so fall back to defaults
    let cfg = config::load().unwrap_or_else(|e| {
        eprintln!("gas: {e:#}; using defaults");
        config::Config::default()
    });
    ui::init(cfg.color);

    let Some(cmd) = Cli::parse().cmd else {
        return tui::run();
    };
    match cmd {
        Cmd::Add {
            id,
            name,
            email,
            github,
            ssh_key,
        } => {
            let p = Profile {
                name,
                email,
                github,
                ssh_key,
            };
            profile::validate(&id, &p)?;
            let mut s = profile::load()?;
            s.insert(id.clone(), p);
            profile::save(&s)?;
            println!("{} profile {}", green("saved"), bold(id));
        }
        Cmd::Rm { id } => {
            let mut s = profile::load()?;
            if s.remove(&id).is_none() {
                bail!("no profile '{id}'");
            }
            profile::save(&s)?;
            println!("removed {id}");
        }
        Cmd::Ls => {
            let s = profile::load()?;
            if s.is_empty() {
                println!("no profiles. add one: gas add work --name \"Me\" --email me@x.com");
            }
            let active = git::git(&["config", "gas.profile"]);
            for (id, p) in &s {
                let mark = if active.as_deref() == Some(id) {
                    green("*")
                } else {
                    " ".into()
                };
                let extra = format!(
                    "gh:{} key:{}",
                    p.github.as_deref().unwrap_or("-"),
                    p.ssh_key.as_deref().unwrap_or("-")
                );
                println!(
                    "{mark} {}  {} <{}>  {}",
                    bold(id),
                    p.name,
                    p.email,
                    dim(extra)
                );
            }
        }
        Cmd::Use { id, global } => println!("{}", profile::apply(&id, global)?),
        Cmd::Status => guard::status()?,
        Cmd::Init => guard::init()?,
        Cmd::Uninstall => guard::uninstall()?,
        Cmd::Config => {
            config::write_default_if_missing()?;
            println!(
                "{}\n{}",
                config::path()?.display(),
                toml::to_string_pretty(&cfg)?
            );
        }
        Cmd::Guard { hook, args } => std::process::exit(guard::run(&hook, &args, &cfg)?),
    }
    Ok(())
}
