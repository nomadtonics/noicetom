# noicetom

A Rust-only app stack with a desktop client, encrypted local storage, and a local server option.

## Included

- `desktop/` — Dioxus desktop application
- `src/` — Rust Axum server using the same encrypted storage format
- `docs/` — static homepage for GitHub Pages download links
- GitHub Actions release workflow for building desktop binaries

## Build and run

Desktop app:

```bash
cargo build --release --manifest-path desktop/Cargo.toml
./target/release/desktop
```

Server:

```bash
cargo build --release
PORT=3000 ./target/release/noicetom
```

## Local encrypted storage

This app stores data in the system data directory as an encrypted file.

- Linux: `~/.local/share/noicetom/messages.json.enc`
- macOS: `~/Library/Application Support/noicetom/messages.json.enc`
- Windows: `%APPDATA%\noicetom\messages.json.enc`

The key is stored in the same directory as `storage.key`, unless you set `NOICETOM_STORAGE_KEY` to a base64-encoded 32-byte key.

## GitHub Release workflow

Push a tag like `v0.1.0` to trigger builds for Linux, macOS, and Windows desktop binaries.

```bash
git tag v0.1.0
git push origin v0.1.0
```

## GitHub Pages

The homepage is in `docs/index.html` and is published through the GitHub Pages workflow.
