# List available commands.
default:
    @just --list

# Build CLI and language server in debug mode.
build:
    cargo build --locked
    cargo build --locked --manifest-path lsp/Cargo.toml

# Build CLI and language server in release mode.
build-release:
    cargo build --locked --release
    cargo build --locked --release --manifest-path lsp/Cargo.toml

# Build only the CLI.
build-cli:
    cargo build --locked

# Build only the language server.
build-lsp:
    cargo build --locked --manifest-path lsp/Cargo.toml

# Run planner, language server, and Agent test suites.
test:
    cargo test --locked
    cargo test --locked --manifest-path lsp/Cargo.toml
    cargo test --locked --manifest-path vendor/agent/Cargo.toml

# Check formatting and warnings in all crates.
check:
    cargo fmt --check
    cargo fmt --check --manifest-path lsp/Cargo.toml
    cargo fmt --check --manifest-path vendor/agent/Cargo.toml
    cargo clippy --locked --all-targets -- -D warnings
    cargo clippy --locked --all-targets --manifest-path lsp/Cargo.toml -- -D warnings
    cargo clippy --locked --all-targets --manifest-path vendor/agent/Cargo.toml -- -D warnings
    RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps
    RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --manifest-path lsp/Cargo.toml
    RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --manifest-path vendor/agent/Cargo.toml

# Install CLI, language server, and Neovim configuration and syntax.
installation:
    cargo install --force --locked --path . --root ~/.local
    cargo install --force --locked --path lsp --root ~/.local
    install -Dm644 lsp/nvim/folplan.lua ~/.config/nvim/lua/plugins/folplan.lua
    install -Dm644 lsp/nvim/syntax/folplan.vim ~/.config/nvim/syntax/folplan.vim

# Build pinned Fast Downward locally for benchmark comparisons.
install-fast-downward:
    #!/usr/bin/env bash
    set -euo pipefail
    revision=9b81c7e422fdf7be9f73b96e9a7a969c483dd5d4
    source_dir="$HOME/.local/share/fast-downward"
    driver="$HOME/.local/bin/fast-downward.py"
    if [[ ! -d "$source_dir/.git" ]]; then
        git clone --depth 1 https://github.com/aibasel/downward.git "$source_dir"
    fi
    git -C "$source_dir" fetch --depth 1 origin "$revision"
    git -C "$source_dir" -c advice.detachedHead=false checkout --detach "$revision"
    cd "$source_dir"
    CC=gcc CXX=g++ CCACHE_DISABLE=1 python3 build.py release
    mkdir -p "$HOME/.local/bin"
    if [[ -e "$driver" && ! -L "$driver" ]] ||
       [[ -L "$driver" && "$(readlink "$driver")" != "$source_dir/fast-downward.py" ]]; then
        printf 'Refusing to replace %s\n' "$driver" >&2
        exit 1
    fi
    ln -sfn "$source_dir/fast-downward.py" "$driver"
