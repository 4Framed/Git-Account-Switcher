//! Terminal output helpers: optional color and the y/N prompt.

use crate::config::Color;
use std::fmt::Display;
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::sync::OnceLock;

static COLOR_ON: OnceLock<bool> = OnceLock::new();

pub fn init(c: Color) {
    let on = match c {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => {
            std::env::var_os("NO_COLOR").is_none()
                && std::io::stdout().is_terminal()
                && std::io::stderr().is_terminal()
        }
    };
    let _ = COLOR_ON.set(on);
}

fn paint(code: &str, s: impl Display) -> String {
    if *COLOR_ON.get().unwrap_or(&false) {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn bold(s: impl Display) -> String {
    paint("1", s)
}
pub fn dim(s: impl Display) -> String {
    paint("2", s)
}
pub fn red(s: impl Display) -> String {
    paint("31", s)
}
pub fn green(s: impl Display) -> String {
    paint("32", s)
}

/// Ask a y/N question on the controlling terminal.
/// `None` means there is no terminal to ask on.
/// Env: `GAS_YES=1` always answers yes; `GAS_NO_TTY=1` pretends no terminal exists.
pub fn confirm(prompt: &str) -> Option<bool> {
    if std::env::var_os("GAS_YES").is_some() {
        return Some(true);
    }
    if std::env::var_os("GAS_NO_TTY").is_some() {
        return None;
    }
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let mut w = &tty;
    write!(w, "{prompt}? [y/N] ").ok()?;
    w.flush().ok()?;
    let mut line = String::new();
    BufReader::new(&tty).read_line(&mut line).ok()?;
    Some(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// Ask, falling back when there is no terminal: `must` blocks, otherwise allow.
pub fn proceed(prompt: &str, must: bool) -> bool {
    confirm(prompt).unwrap_or(!must)
}
