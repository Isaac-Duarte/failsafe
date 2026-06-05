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

## Running

1. Start the server:

```bash
FAILSAFE_JWT_SECRET=your-secret cargo run -p failsafe-sevrer
```

2. On each device, configure `~/.config/failsafe/config.toml`:

```toml
device_id = "your-device-uuid"
device_name = "laptop"
server_url = "http://localhost:8080"
transport = "iroh"
enabled_features = ["clipboard"]
```

3. Log in (use `--register` the first time):

```bash
cargo run -p failsafe-daemon -- login --register --email you@example.com --password your-password
```

4. Run the daemon on each device:

```bash
cargo run -p failsafe-daemon -- run
```

The daemon registers its Iroh public key with the server and polls for peers every 30 seconds. Clipboard sync is enabled between devices when both sides have `clipboard` in `enabled_features`.

One person can have multiple devices, and each device can pick an choose which feature they want to have enabled. As an optional feature, it would be cool to invoke all of these features from the web. Now Iroh does compile to web assembly but it doesn't look like its a direct connection anyway. I think how I would approach this instead is to open that direct connection from the sever itself, but only as a one way for limited features. Ie opening a shell, remote desktop, etc.
