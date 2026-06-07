# Failsafe

Cross-device sync with an Apple-ecosystem feel. A central registration server handles accounts, device pairing, and policy; peer devices communicate over [Iroh](https://iroh.computer/) P2P.

## Features

| Feature | Status |
|---------|--------|
| Clipboard sync | Implemented |
| File send | Implemented |
| Remote shell | Implemented |
| TCP port forwarding | Implemented |
| Notifications, remote desktop, shared drives | Planned |

## Architecture

```
┌─────────────┐     HTTP (auth, devices)     ┌──────────────────┐
│ failsafe    │◄────────────────────────────►│ failsafe-server  │
│ CLI/daemon  │                              │ (Axum + SQLite)  │
└──────┬──────┘                              └────────┬─────────┘
       │ control socket (local)                       │ embedded SPA
       ▼                                              ▼
┌─────────────┐     Iroh P2P (features)      ┌──────────────────┐
│ Feature     │◄────────────────────────────►│ Other devices    │
│ registry    │                              │ (daemons)        │
└─────────────┘                              └──────────────────┘
```

Each device runs a long-lived daemon (`failsafe run`) that registers features (clipboard, send, shell, port forward) and syncs policy from the server. CLI commands like `shell`, `send`, and `port` talk to the daemon over an authenticated local control socket.

## Quick start

The CLI defaults to the public registration server at [https://failsafe.pendejo.dev](https://failsafe.pendejo.dev/). You can register an account and sync devices without running your own server.

### 1. Register and run a device

```bash
failsafe register --email you@example.com
failsafe run
```

Manage devices and pairing codes at [https://failsafe.pendejo.dev](https://failsafe.pendejo.dev/).

On a second machine, generate a pairing code from the web UI, then:

```bash
failsafe pair --code ABCD1234X
failsafe run
```

### 2. Use features

```bash
failsafe send --device laptop ./document.pdf
failsafe shell laptop
failsafe port 8080:3000 --device laptop
```

### Self-hosting (optional)

To run your own registration server:

```bash
cp .env.example .env
# Edit .env — set FAILSAFE_JWT_SECRET and FAILSAFE_ENCRYPTION_KEY

cargo run -p failsafe-server
```

Point clients at it with `--server-url http://127.0.0.1:8080` or `FAILSAFE_SERVER_URL`.

## Configuration

### Server environment variables

| Variable | Required | Description |
|----------|----------|-------------|
| `FAILSAFE_JWT_SECRET` | Yes | Secret for signing JWT access tokens |
| `FAILSAFE_ENCRYPTION_KEY` | Yes | Separate key for encrypting TOTP secrets (must differ from JWT secret) |
| `FAILSAFE_LISTEN` | No | Full listen address, e.g. `0.0.0.0:8080` |
| `FAILSAFE_LISTEN_HOST` | No | Bind host (default `127.0.0.1`) |
| `FAILSAFE_LISTEN_PORT` | No | Bind port (default `8080`) |
| `FAILSAFE_DB_URL` | No | SQLite URL (default: platform data dir) |
| `FAILSAFE_SKIP_WEB_BUILD` | No | Set to `1` to skip npm build when `failsafe-web-ui/dist` exists |
| `RUST_LOG` | No | Log filter, e.g. `info,failsafe_server=debug` |

### Daemon configuration

| Location | Purpose |
|----------|---------|
| `~/.config/failsafe/config.toml` | Device ID, server URL, enabled features |
| `~/.config/failsafe/credentials.toml` | Auth tokens |
| `~/.config/failsafe/control.token` | Local control socket auth token |
| `$XDG_RUNTIME_DIR/failsafe/control.sock` | Daemon control socket (Unix) |

Default server URL is `https://failsafe.pendejo.dev`. Override with `--server-url` or `FAILSAFE_SERVER_URL` when self-hosting.
