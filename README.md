# kagent

kitty-native Agent Lens for monitoring Codex/agent sessions across kitty tabs and windows.

## Install

For normal use, install the `kagent` binary into Cargo's bin directory:

```sh
cargo install --path crates/kagent-cli --locked --force
```

This installs the binary as:

```sh
kagent
```

Make sure Cargo's bin directory is on `PATH`:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

Verify:

```sh
command -v kagent
kagent quick-access
```

`kagent quick-access` is the stable entrypoint intended for kitty quick-access-terminal bindings.

## Development

During development, run from the workspace:

```sh
cargo run -p kagent-cli -- quick-access
cargo run -p kagent-cli -- dash
cargo run -p kagent-cli -- dash --snapshot
```

The dotfiles kitty kitten can fall back to this development command when `kagent` is not installed, but that fallback is only for local development.

## Kitty Quick Access

The recommended kitty binding is:

```conf
map ctrl+a>p kitten kittens/kagent_quick_access.py
```

The kitten runs `kagent quick-access` when `kagent` is installed. For development, override it explicitly:

```sh
export KAGENT_QUICK_ACCESS_COMMAND='cargo run -p kagent-cli -- quick-access'
export KAGENT_QUICK_ACCESS_CWD="$HOME/dev/projects/kagent"
```

## Distribution

Current source distribution:

```sh
git clone https://github.com/ryugen04/kagent.git
cd kagent
cargo install --path crates/kagent-cli --locked --force
```

Future Cargo distribution can use the package name `kagent-cli` with binary `kagent` after crates are published:

```sh
cargo install kagent-cli --locked
```

## Verification

```sh
cargo fmt --check
cargo test
```
