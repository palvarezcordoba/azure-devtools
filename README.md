# Azure DevTools

Rust CLI to inspect Azure DevOps variable groups.

## Development

Requirements:

- Rust nightly (see `rust-toolchain.toml`)
- Cargo (bundled with Rust)

Run the TUI locally:

```bash
cargo run --bin azure_variables -- init # Initialize configuration
cargo run --bin azure_variables -- tui
```

### Tests

```bash
cargo test
```

## Cross-compiling with `cross`

[`cross`](https://github.com/cross-rs/cross) is a drop-in replacement for `cargo` that runs builds inside Docker images with the correct target toolchains already installed.

Install it once:

```bash
cargo install cross
```

### Linux (musl, fully static)

`cross` ships a musl toolchain so you can produce a self-contained binary with no glibc dependency:

```bash
cross build --release --target x86_64-unknown-linux-musl
```

`target/x86_64-unknown-linux-musl/release/azure-devtools` is statically linked and portable across modern distros.

### Windows (GNU, static CRT)

Build a release binary for 64-bit Windows:

```bash
RUSTFLAGS="-C target-feature=+crt-static" cross build --release --target x86_64-pc-windows-gnu
```

Your executable will be written to:

```text
target/x86_64-pc-windows-gnu/release/azure-devtools.exe
```

