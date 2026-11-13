# Git Account Switcher

> [!NOTE]
> This project in its entirity has been created by "vibecoding", due to my lack of knowledge in rust. Contributions are welcomed to fix any issues that may arise as a result of this.

`gas` switches between git and GitHub accounts and shows which account a commit or push comes from
before it goes out, so you don't leak the wrong identity.

## Install

```bash
cargo install --path .
gas init
```

## Use

```bash
gas                                  # interactive picker (a add, e edit, d delete, enter switch)
gas add work --name "Me" --email me@users.noreply.github.com --github my-user --ssh-key ~/.ssh/id_work
gas use work                         # this repo, or global when outside one
gas ls
gas status
```

`gas init` installs git hooks that print the active account on every `git commit` and `git push`
and ask before continuing if something looks wrong.

Settings live in `~/.gas/config.toml`.

## Contributing
Contributions are welcomed. See `CONTRIBUTING.md` for more information.

## License

MIT
