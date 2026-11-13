# Contributing

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs the same three on Linux and macOS.

## Layout

| File | Job |
|------|-----|
| `src/main.rs` | CLI definition and dispatch only |
| `src/config.rs` | `~/.gas/config.toml` schema, defaults, legacy migration |
| `src/profile.rs` | Profile storage, validation, `apply` (writes git config) |
| `src/git.rs` | Wrappers around the `git` binary, shell quoting |
| `src/guard.rs` | Hook install, pre-commit and pre-push logic, `status` |
| `src/tui.rs` | Interactive picker (ratatui) |
| `src/ui.rs` | Color and the y/N prompt |
| `tests/cli.rs` | End-to-end tests using the real binary and real git |

## Adding a config option

1. Add the field to `Config` in `src/config.rs` with a default.
2. Add it to `TEMPLATE`. The `template_matches_defaults` test fails if the two drift.
3. Document it in the README.

## Testing

Tests run in a temp `HOME` with `GAS_HOME` and `GAS_NO_TTY=1` set, so they never touch your real git
config or prompt on your terminal. Copy the `Env` helper in `tests/cli.rs` for new end-to-end tests.

## Rules of thumb

- A hook must never hang or crash a commit. Fall back to defaults and warn.
- Hooks must not call `git rev-parse --git-path hooks/...`: with a global `core.hooksPath` it returns
  gas's own hook and loops forever.
- No tokens or passwords are stored. Keep it that way.
