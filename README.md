# ChatLink

ChatLink is a Windows-hosted, LAN-only text chat for one Windows PC and one iPhone. The desktop app serves an HTTPS progressive web app and a WebSocket endpoint on the same private IPv4 address.

## Requirements

- Node.js 20 or newer and npm
- Rust stable with the MSVC target
- Microsoft C++ Build Tools with **Desktop development with C++**
- Microsoft Edge WebView2 Runtime

The iPhone must be on the same Wi-Fi network. Initial HTTPS setup requires installing the local ChatLink root certificate profile and enabling full trust for it in iOS Certificate Trust Settings.

When Windows Firewall prompts for network access, allow ChatLink on **Private networks only**. Do not enable access on Public networks.

## Development

```powershell
npm.cmd install
npm.cmd run build:protocol
npm.cmd run build:mobile
npm.cmd run dev:desktop
```

To open the mobile UI in a desktop browser during development:

```powershell
npm.cmd run dev:mobile
```

## Build

```powershell
npm.cmd run build
```

The Tauri bundle includes the mobile web build as a resource. SQLite data and the local certificate authority are stored under `%APPDATA%\ChatLink`.

## First iPhone setup

1. Keep the Windows app open and scan the QR code shown in ChatLink.
2. Compare the page fingerprint with the fingerprint shown on Windows.
3. Download the profile, install it in iPhone Settings, then enable full trust under **Settings → General → About → Certificate Trust Settings**.
4. In ChatLink on Windows, select **Đã cài và bật tin cậy chứng chỉ**, scan the updated QR code, and add ChatLink to the Home Screen from Safari.
5. Enter the six-digit access code displayed in ChatLink on Windows.

## Milestone boundaries

- Messages are stored in SQLite before the server acknowledges them.
- The iPhone keeps its unacknowledged outbox in memory until the page closes. Durable mobile history and IndexedDB sync are planned for Milestone 4.
- The temporary access code changes whenever the desktop app starts. Persistent pairing and device tokens are planned for Milestone 5.
- HTTPS certificate installation is required for the iPhone service worker. Only the public CA certificate is included in the downloadable iOS profile; its private key stays protected by Windows DPAPI.
