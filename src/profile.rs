//! Profiles: stored in `~/.gas/profiles.toml`, applied to git config by `apply`.

use crate::{config, git};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub email: String,
    pub github: Option<String>,
    pub ssh_key: Option<String>,
}

pub type Store = BTreeMap<String, Profile>;

pub fn load() -> Result<Store> {
    let p = config::dir()?.join("profiles.toml");
    if !p.exists() {
        return Ok(Store::new());
    }
    toml::from_str(&fs::read_to_string(&p)?).with_context(|| format!("bad file {}", p.display()))
}

pub fn save(s: &Store) -> Result<()> {
    let d = config::dir()?;
    fs::create_dir_all(&d)?;
    fs::write(d.join("profiles.toml"), toml::to_string_pretty(s)?)?;
    Ok(())
}

/// Trim; empty becomes `None`.
pub fn opt(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

pub fn expand(p: &str) -> String {
    match (p.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(h)) => h.join(rest).to_string_lossy().into(),
        _ => p.into(),
    }
}

pub fn validate(id: &str, p: &Profile) -> Result<()> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("id must be letters, digits, '-' or '_'");
    }
    if p.name.trim().is_empty() {
        bail!("name is required");
    }
    if !p.email.contains('@') {
        bail!("email must contain '@'");
    }
    if let Some(k) = &p.ssh_key
        && !Path::new(&expand(k)).exists()
    {
        bail!("ssh key not found: {k}");
    }
    Ok(())
}

pub fn gh_switch(user: &str) -> bool {
    Command::new("gh")
        .args(["auth", "switch", "--user", user])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The `core.sshCommand` value for a key path.
pub fn ssh_command(key: &str) -> String {
    format!(
        "ssh -i {} -o IdentitiesOnly=yes",
        git::sh_quote(&expand(key))
    )
}

/// Switch to profile `id`. Outside a repo, always global. Returns a plain status message.
pub fn apply(id: &str, global: bool) -> Result<String> {
    let s = load()?;
    let p = s
        .get(id)
        .with_context(|| format!("no profile '{id}' (see: gas ls)"))?;
    let global = global || !git::in_repo();
    git::set(global, "user.name", &p.name)?;
    git::set(global, "user.email", &p.email)?;
    git::set(global, "gas.profile", id)?;
    match &p.ssh_key {
        Some(k) => git::set(global, "core.sshCommand", &ssh_command(k))?,
        None => git::unset(global, "core.sshCommand"),
    }
    let scope = if global { "globally" } else { "in this repo" };
    let mut msg = format!("now {id} {scope}: {} <{}>", p.name, p.email);
    if let Some(g) = &p.github {
        if gh_switch(g) {
            msg += &format!("; gh -> {g}");
        } else {
            msg += &format!("; gh switch to {g} failed (run `gh auth login`)");
        }
    }
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(email: &str) -> Profile {
        Profile {
            name: "N".into(),
            email: email.into(),
            ..Default::default()
        }
    }

    #[test]
    fn validates_fields() {
        assert!(validate("work", &p("a@b.c")).is_ok());
        assert!(validate("", &p("a@b.c")).is_err());
        assert!(validate("a b", &p("a@b.c")).is_err());
        assert!(validate("work", &p("nope")).is_err());
        let mut bad = p("a@b.c");
        bad.ssh_key = Some("/definitely/missing".into());
        assert!(validate("work", &bad).is_err());
    }

    #[test]
    fn ssh_command_quotes_path() {
        assert_eq!(ssh_command("/k y"), "ssh -i '/k y' -o IdentitiesOnly=yes");
    }

    #[test]
    fn opt_trims() {
        assert_eq!(opt("  "), None);
        assert_eq!(opt(" x "), Some("x".into()));
    }
}
