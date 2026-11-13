//! Thin wrappers around the `git` binary.

use anyhow::{Result, bail};
use std::process::Command;

/// Run git, return trimmed stdout on success.
pub fn git(args: &[&str]) -> Option<String> {
    let o = Command::new("git").args(args).output().ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

pub fn set(global: bool, key: &str, val: &str) -> Result<()> {
    let scope = if global { "--global" } else { "--local" };
    if !Command::new("git")
        .args(["config", scope, key, val])
        .status()?
        .success()
    {
        bail!("git config {scope} {key} failed");
    }
    Ok(())
}

pub fn unset(global: bool, key: &str) {
    let scope = if global { "--global" } else { "--local" };
    let _ = Command::new("git")
        .args(["config", scope, "--unset", key])
        .status();
}

pub fn in_repo() -> bool {
    git(&["rev-parse", "--git-dir"]).is_some()
}

/// Quote for a POSIX shell (git runs `core.sshCommand` and hooks through `sh`).
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_spaces_and_apostrophes() {
        assert_eq!(sh_quote("/a b/key"), "'/a b/key'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
    }
}
