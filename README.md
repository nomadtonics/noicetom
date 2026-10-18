# noicetom — privacy-first encrypted messaging server

This repository contains a minimal privacy-first encrypted messaging server in Rust.

Design highlights
- End-to-end encrypted messages: server stores only encrypted symmetric keys and ciphertext; it never has access to plaintext.
- Key-based auth: users register an Ed25519 public key and authenticate by signing a challenge.
- Hybrid encryption: each message encrypted with an ephemeral symmetric key; key encrypted to recipient's public key.
- SQLite for storage (no external DB required).

Quickstart (development)
1) Install Rust toolchain (rustup)
2) Build and run:
   cargo build --release
   NOICETOM_STORAGE_PATH=./data/noicetom.sqlite SERVER_SECRET="your-secret" cargo run --release

3) Use the API (see README.md for endpoints and example flows).
