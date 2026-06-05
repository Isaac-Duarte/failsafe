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

One person can have multiple devices, and each device can pick an choose which feature they want to have enabled. As an optional feature, it would be cool to invoke all of these features from the web. Now Iroh does compile to web assembly but it doesn't look like its a direct connection anyway. I think how I would approach this instead is to open that direct connection from the sever itself, but only as a one way for limited features. Ie opening a shell, remote desktop, etc.

# TODO

- A better readme

## Rich clipboard sync

The clipboard feature syncs text, HTML, images, and files between paired devices.

- **Text** and small **HTML** (under 64 KiB) are sent inline over the failsafe transport.
- **Large HTML**, **images**, and **file contents** are stored in a local iroh-blobs store and transferred by BLAKE3 hash over the blob protocol.
- On receive, files are written to `~/.cache/failsafe/clipboard/<session>/` and placed on the clipboard as file paths.

### Configuration

In `~/.config/failsafe/config.toml`:

```toml
# Optional: override blob store location (default: ~/.local/share/failsafe/blobs)
# blob_store_path = "/var/lib/failsafe/blobs"

# Max size per clipboard file/blob (default: 104857600 = 100 MiB)
clipboard_max_file_bytes = 104857600
```

Device sync always uses the Iroh transport; there is no mock/in-memory transport at runtime.
