//! `~/.gas/config.toml`: user-tunable behaviour. Profiles live next to it in `profiles.toml`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum PushPrompt {
    /// Show identity and ask on every push.
    #[default]
    Always,
    /// Ask only when a pushed commit carries a different identity.
    Mismatch,
    /// Show identity, never ask.
    Never,
}

#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum CommitPrompt {
    /// Ask when the author email matches no saved profile, or the author is carried over.
    #[default]
    Unknown,
    /// Ask on every commit.
    Always,
    /// Print the identity, never ask.
    Never,
}

#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    pub push_prompt: PushPrompt,
    pub commit_prompt: CommitPrompt,
    /// Run `gh auth switch` for the repo's profile before each push.
    pub gh_auto_switch: bool,
    pub color: Color,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            push_prompt: PushPrompt::default(),
            commit_prompt: CommitPrompt::default(),
            gh_auto_switch: true,
            color: Color::default(),
        }
    }
}

/// Written to disk by `gas init` / `gas config`. Must parse to `Config::default()` (tested).
pub const TEMPLATE: &str = r#"# gas configuration

# git push: "always" asks every time, "mismatch" asks only when a pushed commit
# has a different author, "never" only prints the identity.
push_prompt = "always"

# git commit: "unknown" asks when the author email matches no saved profile,
# "always" asks every time, "never" only prints the identity.
commit_prompt = "unknown"

# Run `gh auth switch` for the repo's profile before each push (HTTPS auth).
gh_auto_switch = true

# "auto", "always" or "never".
color = "auto"
"#;

/// Data dir. Override with `GAS_HOME` (used by tests).
pub fn dir() -> Result<PathBuf> {
    if let Some(h) = std::env::var_os("GAS_HOME") {
        return Ok(h.into());
    }
    Ok(dirs::home_dir().context("no home dir")?.join(".gas"))
}

pub fn path() -> Result<PathBuf> {
    Ok(dir()?.join("config.toml"))
}

pub fn load() -> Result<Config> {
    let p = path()?;
    if !p.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(&p)?;
    toml::from_str(&text).with_context(|| format!("bad config {}", p.display()))
}

pub fn write_default_if_missing() -> Result<()> {
    let p = path()?;
    if !p.exists() {
        fs::create_dir_all(dir()?)?;
        fs::write(p, TEMPLATE)?;
    }
    Ok(())
}

fn legacy_dir() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(".config/gas"))
}

/// Pre-0.2 data lived in `~/.config/gas`. Move it and repoint the global hook path.
pub fn migrate_legacy() {
    if std::env::var_os("GAS_HOME").is_some() {
        return;
    }
    let (Ok(new), Some(old)) = (dir(), legacy_dir()) else {
        return;
    };
    if new.exists() || !old.exists() || fs::rename(&old, &new).is_err() {
        return;
    }
    let old_hooks = old.join("hooks");
    if crate::git::git(&["config", "--global", "core.hooksPath"]).as_deref() == old_hooks.to_str() {
        let _ = crate::git::set(true, "core.hooksPath", &new.join("hooks").to_string_lossy());
    }
    eprintln!("gas: moved {} -> {}", old.display(), new.display());
}

/// Is `p` a hooks dir gas installed (current or legacy location)?
pub fn is_our_hooks_dir(p: &str) -> bool {
    let p = std::path::Path::new(p);
    dir().map(|d| p == d.join("hooks")).unwrap_or(false)
        || legacy_dir().map(|d| p == d.join("hooks")).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_matches_defaults() {
        assert_eq!(
            toml::from_str::<Config>(TEMPLATE).unwrap(),
            Config::default()
        );
    }

    #[test]
    fn partial_file_fills_defaults() {
        let c: Config = toml::from_str("push_prompt = \"never\"").unwrap();
        assert_eq!(c.push_prompt, PushPrompt::Never);
        assert!(c.gh_auto_switch);
    }

    #[test]
    fn bad_value_is_an_error() {
        assert!(toml::from_str::<Config>("color = \"purple\"").is_err());
    }
}
