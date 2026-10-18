# noicetom - Dioxus frontend

This is a simple Dioxus (WASM) frontend that connects to the Axum backend via WebSocket (/ws).

Build with trunk:

```bash
cd frontend
trunk serve
```

By default the frontend expects the backend to be available on the same host (same origin). If you serve the backend separately, ensure CORS and the correct host/port for ws/wss are used.
