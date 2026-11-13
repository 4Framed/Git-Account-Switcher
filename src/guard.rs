//! Global git hooks (pre-push, pre-commit) and `gas status`.

use crate::config::{self, CommitPrompt, PushPrompt};
use crate::ui::{bold, dim, green, proceed, red};
use crate::{git, profile};
use anyhow::{Result, bail};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const HOOKS: [&str; 2] = ["pre-push", "pre-commit"];

pub fn hooks_dir() -> Result<PathBuf> {
    Ok(config::dir()?.join("hooks"))
}

fn guard_is_on() -> bool {
    git::git(&["config", "--global", "core.hooksPath"])
        .is_some_and(|p| config::is_our_hooks_dir(&p))
}

pub fn init() -> Result<()> {
    let dir = hooks_dir()?;
    let dir_s = dir.to_string_lossy().into_owned();
    if let Some(cur) = git::git(&["config", "--global", "core.hooksPath"])
        && !config::is_our_hooks_dir(&cur)
    {
        bail!("core.hooksPath already set to {cur}; unset it first");
    }
    fs::create_dir_all(&dir)?;
    let exe = std::env::current_exe()?;
    for name in HOOKS {
        let hook = dir.join(name);
        let script = format!(
            "#!/bin/sh\nexec {} guard {name} \"$@\"\n",
            git::sh_quote(&exe.to_string_lossy())
        );
        fs::write(&hook, script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
        }
    }
    git::set(true, "core.hooksPath", &dir_s)?;
    config::write_default_if_missing()?;
    println!(
        "{}: `git commit` and `git push` now show which account they use.",
        green("guard on")
    );
    println!(
        "{}",
        dim("binary path is baked into the hooks; re-run `gas init` if you move it")
    );
    Ok(())
}

pub fn uninstall() -> Result<()> {
    if guard_is_on() {
        git::unset(true, "core.hooksPath");
        println!("guard removed");
    } else {
        println!("guard not installed");
    }
    Ok(())
}

pub fn status() -> Result<()> {
    let name = git::git(&["config", "user.name"]).unwrap_or_default();
    let email = git::git(&["config", "user.email"]).unwrap_or_default();
    let prof = git::git(&["config", "gas.profile"]);
    println!("{}  {name} <{email}>", bold("identity"));
    println!(
        "{}   {}",
        bold("profile"),
        prof.as_deref().unwrap_or("(none)")
    );
    if let Some(k) = git::git(&["config", "core.sshCommand"]) {
        println!("{}       {k}", bold("ssh"));
    }
    if let Some(r) = git::git(&["remote", "-v"]) {
        println!("{}\n{r}", bold("remotes"));
    }
    let g = if guard_is_on() {
        green("on")
    } else {
        format!("{} (run: gas init)", red("off"))
    };
    println!("{}     {g}", bold("guard"));
    Ok(())
}

/// Entry point for `gas guard <hook> ...`. Returns the hook's exit code.
pub fn run(hook: &str, args: &[String], cfg: &config::Config) -> Result<i32> {
    match hook {
        "pre-push" if args.len() >= 2 => push(&args[0], &args[1], cfg),
        "pre-commit" => commit(cfg),
        _ => Ok(0),
    }
}

/// From pre-push stdin lines `<local ref> <local sha> <remote ref> <remote sha>`,
/// return `(local sha, remote sha)` pairs, skipping branch deletions.
pub fn parse_push_refs(stdin: &str) -> Vec<(String, String)> {
    let zeros = |s: &str| s.chars().all(|c| c == '0');
    stdin
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (f.len() >= 4 && !zeros(f[1])).then(|| (f[1].to_string(), f[3].to_string()))
        })
        .collect()
}

/// `git log` args listing the commits a push would publish.
pub fn log_args(remote: &str, local: &str, remote_sha: &str) -> Vec<String> {
    let fmt = "--format=%an <%ae>%n%cn <%ce>".to_string();
    if remote_sha.chars().all(|c| c == '0') {
        vec![
            "log".into(),
            fmt,
            local.into(),
            "--not".into(),
            format!("--remotes={remote}"),
        ]
    } else {
        vec!["log".into(), fmt, format!("{remote_sha}..{local}")]
    }
}

fn push(remote: &str, url: &str, cfg: &config::Config) -> Result<i32> {
    let mut stdin = String::new();
    std::io::stdin().read_to_string(&mut stdin)?;

    let email = git::git(&["config", "user.email"]).unwrap_or_default();
    let name = git::git(&["config", "user.name"]).unwrap_or_default();
    let prof = git::git(&["config", "gas.profile"]);

    if cfg.gh_auto_switch
        && let Some(g) = prof
            .as_deref()
            .and_then(|p| profile::load().ok()?.get(p)?.github.clone())
    {
        profile::gh_switch(&g);
    }

    let mut authors = BTreeSet::new();
    for (local, remote_sha) in parse_push_refs(&stdin) {
        let args = log_args(remote, &local, &remote_sha);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Some(out) = git::git(&args) {
            authors.extend(out.lines().filter(|l| !l.is_empty()).map(String::from));
        }
    }
    let bad: Vec<_> = authors
        .iter()
        .filter(|a| !a.contains(&format!("<{email}>")))
        .collect();
    let mismatch = !bad.is_empty();

    eprintln!("{}", bold("── gas: push check ──"));
    eprintln!("to        {url}");
    eprintln!(
        "as        {}  {}",
        bold(format!("{name} <{email}>")),
        dim(format!("profile: {}", prof.as_deref().unwrap_or("none")))
    );
    for a in &authors {
        let line = format!("commit by {a}");
        eprintln!(
            "{}",
            if bad.contains(&a) {
                red(line)
            } else {
                green(line)
            }
        );
    }
    if mismatch {
        eprintln!(
            "{}",
            red(bold(
                "! commits carry an identity that is not your current one"
            ))
        );
    }

    let ask = match cfg.push_prompt {
        PushPrompt::Always => true,
        PushPrompt::Mismatch => mismatch,
        PushPrompt::Never => false,
    };
    if ask && !proceed(if mismatch { "push anyway" } else { "continue" }, mismatch) {
        eprintln!("push aborted");
        return Ok(1);
    }
    chain("pre-push", &[remote, url], &stdin)
}

fn commit(cfg: &config::Config) -> Result<i32> {
    let ident = git::git(&["var", "GIT_AUTHOR_IDENT"]).unwrap_or_default();
    let email = ident
        .split_once('<')
        .and_then(|(_, r)| r.split_once('>'))
        .map(|(e, _)| e)
        .unwrap_or("");
    let name = ident.split('<').next().unwrap_or("").trim();
    let known = profile::load()?.values().any(|p| p.email == email);
    let cfg_email = git::git(&["config", "user.email"]).unwrap_or_default();
    let carried = email != cfg_email; // --amend / cherry-pick keep the old author
    let prof = git::git(&["config", "gas.profile"]);

    let line = format!("committing as {name} <{email}>");
    eprintln!(
        "{} {} {}",
        bold("gas:"),
        if known { green(line) } else { red(line) },
        dim(format!("profile: {}", prof.as_deref().unwrap_or("none")))
    );
    if carried {
        eprintln!(
            "{}",
            red(bold(format!(
                "! author is kept from an earlier commit, not your current {cfg_email}"
            )))
        );
        eprintln!("  fix: git commit --amend --reset-author");
    }
    if !known {
        eprintln!(
            "{} (switch with `gas`)",
            red(bold("! this email matches no saved profile"))
        );
    }

    let needs = !known || carried;
    let ask = match cfg.commit_prompt {
        CommitPrompt::Unknown => needs,
        CommitPrompt::Always => true,
        CommitPrompt::Never => false,
    };
    if ask && !proceed("commit anyway", needs) {
        eprintln!("commit aborted");
        return Ok(1);
    }
    chain("pre-commit", &[], "")
}

/// Run the repo's own hook. A global `core.hooksPath` shadows `.git/hooks`.
fn chain(hook: &str, args: &[&str], stdin: &str) -> Result<i32> {
    // not `--git-path hooks/..`: with core.hooksPath set, that returns our own hook dir (infinite loop)
    let Some(d) = git::git(&["rev-parse", "--git-common-dir"]) else {
        return Ok(0);
    };
    let p = PathBuf::from(d).join("hooks").join(hook);
    if fs::metadata(&p).is_err() {
        return Ok(0);
    }
    let mut c = Command::new(&p).args(args).stdin(Stdio::piped()).spawn()?;
    let _ = c.stdin.take().map(|mut s| s.write_all(stdin.as_bytes()));
    Ok(c.wait()?.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const Z: &str = "0000000000000000000000000000000000000000";

    #[test]
    fn parses_refs_and_skips_deletes() {
        let input =
            format!("refs/heads/a aaa refs/heads/a bbb\nrefs/heads/b {Z} refs/heads/b ccc\n");
        assert_eq!(
            parse_push_refs(&input),
            vec![("aaa".to_string(), "bbb".to_string())]
        );
    }

    #[test]
    fn log_range_for_new_vs_existing_branch() {
        assert_eq!(log_args("origin", "aaa", "bbb")[2], "bbb..aaa");
        assert_eq!(log_args("origin", "aaa", Z)[4], "--remotes=origin");
    }
}
