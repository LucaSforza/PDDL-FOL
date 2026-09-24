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

# Run both test suites.
test:
    cargo test --locked
    cargo test --locked --manifest-path lsp/Cargo.toml

# Check formatting and warnings in both crates.
check:
    cargo fmt --check
    cargo fmt --check --manifest-path lsp/Cargo.toml
    cargo clippy --locked --all-targets -- -D warnings
    cargo clippy --locked --all-targets --manifest-path lsp/Cargo.toml -- -D warnings

# Install CLI, language server, and Neovim configuration.
installation:
    cargo install --force --locked --path . --root ~/.local
    cargo install --force --locked --path lsp --root ~/.local
    install -Dm644 lsp/nvim/folplan.lua ~/.config/nvim/lua/plugins/folplan.lua
