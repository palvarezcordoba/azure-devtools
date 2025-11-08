# Azure DevTools

CLI to inspect Azure DevOps variable groups.

# Demo
![azure-devtools-demo](./demo.gif)

# Authentication
Install the Azure CLI from https://learn.microsoft.com/en-us/cli/azure/install-azure-cli and authenticate with:

```bash
az login
```

## Installation

Download a prebuilt binary from the [GitHub Releases](https://github.com/palvarezcordoba/azure-devtools/releases) page whenever a new tag is published.

| Platform | Asset | Notes |
| --- | --- | --- |
| Linux (glibc) | `azure-devtools-x86_64-unknown-linux-gnu.tar.gz` | Works on most modern distros |
| Linux (musl) | `azure-devtools-x86_64-unknown-linux-musl.tar.gz` | Fully static, no glibc dependency |
| Windows | `azure-devtools-x86_64-pc-windows-msvc.zip` | Built with MSVC toolchain |

1. Extract the archive (e.g., `tar -xzf azure-devtools-x86_64-unknown-linux-gnu.tar.gz`).
2. Move the extracted binary (e.g., `azure-devtools-x86_64-unknown-linux-gnu`) somewhere on your `PATH`, renaming it to `azure-devtools` for example.

## Development

Requirements:

- Rust nightly (see `rust-toolchain.toml`)

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
