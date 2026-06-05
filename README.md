# What is this
I want to have software where somebody can download it on multiple machines and it provides an apple-like experience when it comes to device syncing.

# Ideas I have
- Clipboard Syncing
- Copy and paste files
- Notification Syncing
- Shell Spawning
- Laptop as monitor
- Screen sharing
- Remote desktop
- TCP Reverse Proxy
- Virtual Shared Drive
- Camera / Microphone handoff
- Shared media controls

# Rough idea on how I will do it
I want to have a trait-based approach for each one of these features. For testing purposes I think I want to abstract all Iroh specific stuff to these, however, I think this may not be feasible?

I want a main web sever that will handle account & device registration.

## Install

```bash
cargo install --path crates/failsafe
cargo install --path crates/failsafe-server
```

Building `failsafe-server` requires Node.js 20+ (the Rust build runs `npm ci` and `npm run build` in `failsafe-web` automatically).

## Running

### 1. Start the server

Copy [`.env.example`](.env.example) to `.env` and set `FAILSAFE_JWT_SECRET`, then:

```bash
failsafe-server
```

Or pass variables inline:

```bash
FAILSAFE_JWT_SECRET=your-secret failsafe-server
```

From the repo without installing:

```bash
cargo build --release -p failsafe-server
./target/release/failsafe-server
```

Both `failsafe-server` and `failsafe` load a `.env` file from the current working directory when present.

Configure the server listen address via `.env`, flags, or both (CLI wins):

```bash
# .env
FAILSAFE_LISTEN_HOST=0.0.0.0
FAILSAFE_LISTEN_PORT=8080

# or a single variable
FAILSAFE_LISTEN=0.0.0.0:8080

# or CLI flags
failsafe-server --host 0.0.0.0 --port 8080
failsafe-server --listen 0.0.0.0:8080
```

Open `http://localhost:8080` for the web UI (register, log in, view devices, generate pairing codes).

To skip the frontend rebuild during Rust iteration (when `failsafe-web/dist` already exists):

```bash
FAILSAFE_SKIP_WEB_BUILD=1 cargo build -p failsafe-server
```

### 2. Authenticate on each device

You can use the **web UI** or the **CLI**. Credentials from either path are saved to `~/.config/failsafe/credentials.toml` when using the CLI.

**Web UI:** register or log in at `http://localhost:8080`, then use **Add this device** to generate a pairing code.

**CLI — first device (create an account):**

```bash
failsafe register --email you@example.com --password your-password
```

**CLI — returning user:**

```bash
failsafe login --email you@example.com --password your-password
```

### 3. Run the daemon

```bash
failsafe run
```

The daemon registers its Iroh public key with the server and polls for peers every 30 seconds. Clipboard sync is enabled between devices when both sides have `clipboard` in `enabled_features`.

### Adding another device

You can add a device with **login**, **pairing via CLI**, or a **pairing code from the web UI**.

**Option A — log in directly** (if you have the account password):

```bash
failsafe login --email you@example.com --password your-password
failsafe run
```

**Option B — pair with a code** (no password needed on the new device):

On a device that is already authenticated, generate a code:

```bash
failsafe pair
```

Or generate a code from the web UI (**Devices → Add this device**).

On the new device:

```bash
failsafe pair --code A3K9Z1
failsafe run
```

Optionally set a device name when pairing:

```bash
failsafe pair --code A3K9Z1 --name laptop
```

## Web UI development

For frontend hot reload without rebuilding Rust:

```bash
# terminal 1
FAILSAFE_JWT_SECRET=your-secret cargo run -p failsafe-server

# terminal 2
cd failsafe-web && npm run dev
```

Open `http://localhost:5173` (Vite proxies `/api` to the server).

One person can have multiple devices, and each device can pick an choose which feature they want to have enabled. As an optional feature, it would be cool to invoke all of these features from the web. Now Iroh does compile to web assembly but it doesn't look like its a direct connection anyway. I think how I would approach this instead is to open that direct connection from the sever itself, but only as a one way for limited features. Ie opening a shell, remote desktop, etc.
