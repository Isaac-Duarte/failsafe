# RustDesk `scrap` fallback (optional)

Use this path if xcap/JPEG quality is insufficient or Windows capture is needed before iroh-live adds it.

## When to consider

- iroh-live remains blocked on iroh 1.0 for an extended period
- Wayland/DXGI capture edge cases xcap does not handle
- Hardware H.264/H.265 via NVENC/VAAPI is required at RustDesk parity

## Steps (not implemented by default)

1. Vendor RustDesk workspace crates as git submodules or path deps:
   - `libs/scrap` — capture + encode
   - `libs/hbb_common` — required by scrap (protobuf types, platform helpers)
   - `hwcodec` (git) — optional hardware encode
2. Initialize submodule: `git submodule update --init libs/hbb_common`
3. Build deps via vcpkg: `libvpx`, `libyuv`, `aom`, `opus`
4. **Decouple transport:** use scrap only for `Capturer` + `Encoder` output; push encoded NAL units over Failsafe's `FDT1` stream (frame type `2 = h264`) instead of `hbb_common::VideoFrame` over TCP.
5. **AGPL:** vendoring `hbb_common` triggers AGPL-3.0 obligations for distributed builds.

## What not to vendor

- `hbb_common` TCP/WebSocket framing — incompatible with Iroh
- RustDesk `enigo` fork — use upstream `enigo` (already in failsafe-desktop for input)
- RustDesk `clipboard` — Failsafe already has `failsafe-clipboard`

## Reference files in RustDesk

- `src/server/video_service.rs` — capture loop, QoS, display switching
- `libs/scrap/src/common/codec.rs` — encoder factory
- `libs/scrap/src/wayland/` — PipeWire Wayland capture
