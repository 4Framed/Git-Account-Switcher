//! End-to-end tests: run the real binary and real git in a throwaway HOME.

use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

struct Env {
    home: TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
        }
    }

    fn cmd(&self, prog: &str, dir: &Path) -> Command {
        let mut c = Command::new(prog);
        c.current_dir(dir)
            .env("HOME", self.home.path())
            .env("GAS_HOME", self.home.path().join(".gas"))
            .env("GAS_NO_TTY", "1") // never prompt on the dev's real terminal
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GAS_YES");
        c
    }

    fn gas(&self, dir: &Path, args: &[&str]) -> Output {
        self.cmd(env!("CARGO_BIN_EXE_gas"), dir)
            .args(args)
            .output()
            .unwrap()
    }

    fn git(&self, dir: &Path, args: &[&str]) -> Output {
        self.cmd("git", dir).args(args).output().unwrap()
    }
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

#[test]
fn use_outside_repo_is_global() {
    let e = Env::new();
    let h = e.home.path();
    assert!(
        e.gas(h, &["add", "work", "--name", "W", "--email", "w@x.com"])
            .status
            .success()
    );
    let u = e.gas(h, &["use", "work"]);
    assert!(u.status.success(), "{}", String::from_utf8_lossy(&u.stderr));
    assert_eq!(
        out(&e.git(h, &["config", "--global", "user.email"])),
        "w@x.com"
    );
}

#[test]
fn add_rejects_bad_email() {
    let e = Env::new();
    let o = e.gas(
        e.home.path(),
        &["add", "x", "--name", "N", "--email", "nope"],
    );
    assert!(!o.status.success());
}

#[test]
fn commit_guard_blocks_unknown_email_without_tty() {
    let e = Env::new();
    let h = e.home.path();
    e.gas(h, &["add", "work", "--name", "W", "--email", "w@x.com"]);
    assert!(e.gas(h, &["init"]).status.success());
    let repo = h.join("repo");
    std::fs::create_dir(&repo).unwrap();
    e.git(&repo, &["init", "-q"]);

    // identity matches a profile: allowed
    e.gas(&repo, &["use", "work"]);
    assert!(
        e.git(&repo, &["commit", "-q", "--allow-empty", "-m", "a"])
            .status
            .success()
    );

    // identity matches no profile: blocked
    e.git(&repo, &["config", "user.email", "stranger@x.com"]);
    let o = e.git(&repo, &["commit", "-q", "--allow-empty", "-m", "b"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("matches no saved profile"));
}

#[test]
fn commit_prompt_never_only_warns() {
    let e = Env::new();
    let h = e.home.path();
    e.gas(h, &["init"]);
    std::fs::write(h.join(".gas/config.toml"), "commit_prompt = \"never\"\n").unwrap();
    let repo = h.join("repo");
    std::fs::create_dir(&repo).unwrap();
    e.git(&repo, &["init", "-q"]);
    e.git(&repo, &["config", "user.email", "stranger@x.com"]);
    e.git(&repo, &["config", "user.name", "S"]);
    assert!(
        e.git(&repo, &["commit", "-q", "--allow-empty", "-m", "a"])
            .status
            .success()
    );
}

#[test]
fn repo_hook_still_runs_and_can_veto() {
    let e = Env::new();
    let h = e.home.path();
    e.gas(h, &["add", "work", "--name", "W", "--email", "w@x.com"]);
    e.gas(h, &["init"]);
    let repo = h.join("repo");
    std::fs::create_dir(&repo).unwrap();
    e.git(&repo, &["init", "-q"]);
    e.gas(&repo, &["use", "work"]);
    let hook = repo.join(".git/hooks/pre-commit");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(
        !e.git(&repo, &["commit", "-q", "--allow-empty", "-m", "a"])
            .status
            .success()
    );
}
