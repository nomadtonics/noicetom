# noicetom

A complete Rust-based API for the noicetom application. This project provides a lightweight HTTP service for health checks, version reporting, and storing simple chat messages in memory.

## Features

- Rust HTTP API built with Axum
- Health and version endpoints
- In-memory message storage
- Validation for chat messages
- Graceful shutdown on Ctrl+C and SIGTERM
- Unit tests for API routes

## Requirements

- Rust stable toolchain
- Cargo

## Quick start

```bash
cargo run
```

The service listens on `http://0.0.0.0:3000` by default. You can override the port with:

```bash
PORT=8080 cargo run
```

## Endpoints

- `GET /` - Root metadata
- `GET /health` - Health check
- `GET /api/health` - Health check
- `GET /api/version` - App version
- `GET /api/messages` - List all messages
- `POST /api/messages` - Create a message

Example:

```bash
curl http://localhost:3000/api/health
curl http://localhost:3000/api/messages
curl -X POST http://localhost:3000/api/messages \
  -H 'Content-Type: application/json' \
  -d '{"sender":"alice","content":"hello from noicetom"}'
```

## Testing

```bash
cargo test
```

## License

MIT
