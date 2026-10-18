# wasm-client

This directory is a placeholder for a Rust/WASM browser client that demonstrates key generation, registration, challenge signing, and encrypt/decrypt flows.

For the browser client we recommend using libsodium.js for the crypto primitives (x25519/curve25519, chacha20-poly1305) or using the Rust `wasm-bindgen` + `sodiumoxide` wrapper.

Steps (high level):
- Build the server and run it locally
- Use wasm-pack to build the client and serve the static files
- Implement keypair generation and register/public-key upload
- Implement auth challenge flow: fetch challenge, sign, verify
- Implement hybrid encryption: generate ephemeral symmetric key, encrypt message, encrypt ephemeral key to recipient public key, POST to /api/messages/send
