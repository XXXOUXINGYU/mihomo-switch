# Mihomo Switch

A desktop client for the [mihomo](https://github.com/MetaCubeX/mihomo) (Clash.Meta) proxy core, built with Tauri 2 + React 18 + Rust.

Unlike conventional Clash GUIs that expose a single proxy port and switch the active node, **Mihomo Switch runs a pool of fixed local ports in parallel**. Each port slot is bound to a chosen node, and every bound port is live at the same time — useful for traffic splitting, multi-account workflows, and automation that needs several independent proxy chains at once.

> Status: v0.6.0 — Windows-first, actively used as a personal tool.

---

## Features

- **Port-slot pool model** — define any number of fixed local ports (e.g. `10801`, `10802`), each independently bound to a node. All enabled ports run concurrently from a single mihomo process.
- **Fingerprint-based bindings** — slots bind to a transport fingerprint instead of a node index, so bindings survive subscription re-imports and reordering without drifting to the wrong node.
- **Multi-format subscription parsing** — Clash YAML, sing-box JSON, Base64 URI lists, and plain URI schemes (`vless`, `vmess`, `trojan`, `ss`) with automatic detection and multi-UA retry.
- **Hot reload** — switch nodes or rebind ports without restarting the core, via mihomo's controller API.
- **Per-port & per-node traffic monitoring** — live connections are attributed back to their inbound local port and resolved node, with session history.
- **Latency testing** — batch test node latency against a reachability endpoint.
- **Robust process lifecycle** — Windows Job Object ensures the core is reaped even if the app crashes; controller readiness is polled before reporting "running".
- **System tray** — minimize to tray, show/quit from the tray menu.
- **Atomic persistence** — settings are written atomically with corruption backup and schema migration.
- **Security-conscious** — generated listeners are bound to `127.0.0.1` only; the controller listens on a random local port with a per-run secret.

## Tech Stack

| Layer | Stack |
|---|---|
| Desktop framework | Tauri 2 |
| Backend | Rust 2021 (`anyhow`, `reqwest`, `serde`, `tokio`, `serde_yaml`) |
| Frontend | React 18 + TypeScript |
| Build tooling | Vite 8 |
| Testing | Rust `#[test]` + Vitest + jsdom |
| Installer | NSIS (custom build script) |

## Prerequisites

- [Node.js](https://nodejs.org/) (LTS)
- [Rust](https://www.rust-lang.org/) toolchain (edition 2021, `rust-version = 1.77.2`)
- [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) (on Windows: Microsoft C++ Build Tools + WebView2)
- **`mihomo.exe`** — the mihomo core binary. Because the binary is large, it is **not** included in this repository. Download it from the [mihomo releases](https://github.com/MetaCubeX/mihomo/releases) and place it at the **project root** (`./mihomo.exe`). The Tauri bundle references it as a resource, so both development and packaging require it to be present there.

## Getting Started

```bash
# 1. Install frontend dependencies
npm install

# 2. Place the core binary at the project root
#    (download mihomo.exe and put it next to package.json)

# 3. Run the desktop app in development
npm run tauri dev
```

`npm run tauri dev` automatically starts the Vite dev server (`beforeDevCommand`) and launches the Tauri window.

### Frontend-only development

```bash
npm run dev      # Vite dev server at http://localhost:1420
```

The UI has a browser-preview mode so components can be developed and tested without the Tauri runtime.

## Building

```bash
# Frontend production build
npm run build

# Windows NSIS installer (runs the full release pipeline + checksums)
npm run build:desktop
```

`scripts/build-nsis.ps1` performs the release build, patches the generated NSIS script (custom icon, strips internal debug binaries, adds runtime-data cleanup on uninstall), rebuilds the installer, and emits `SHA256SUMS.txt`.

## Testing

```bash
# Frontend tests (Vitest + jsdom)
npm test

# Rust tests
cargo test --manifest-path src-tauri/Cargo.toml
```

## Project Structure

```
mihomo_switch/
├── src/                     # Frontend (React + TypeScript)
│   ├── ui/
│   │   ├── App.tsx          # Top-level shell: status bar, nav, page routing
│   │   ├── pages/           # Ports, NodeLibrary, Subscriptions, Activity, Settings
│   │   ├── components/      # Tables, dialogs, panels, navigation rail
│   │   ├── hooks/           # useAppData, useTheme, useToast, ...
│   │   └── tauri.ts         # Centralized IPC bridge (all invoke() calls)
│   └── main.tsx
├── src-tauri/               # Backend (Rust)
│   ├── src/
│   │   ├── lib.rs           # Tauri builder, tray, window events, command registration
│   │   ├── commands.rs      # Tauri command handlers
│   │   ├── models.rs        # Data models & serialization
│   │   ├── settings.rs      # Persistence, migration, port-slot CRUD
│   │   ├── parser.rs        # Subscription parsing (Clash / sing-box / URI)
│   │   ├── config.rs        # mihomo pool-config generation (atomic commit)
│   │   ├── runner.rs        # Core process management (Job Object, hot reload)
│   │   ├── traffic.rs       # Connection/traffic monitoring & node attribution
│   │   └── latency.rs       # Node latency testing
│   ├── capabilities/        # Tauri permission capabilities
│   └── tauri.conf.json
└── scripts/build-nsis.ps1   # Windows release packaging
```

## How It Works

1. **Subscriptions** are imported (fetched or pasted) and parsed into a normalized node list.
2. **Port slots** are user-defined local ports. Each slot binds to one node via a transport fingerprint.
3. On **start**, Mihomo Switch collects every enabled slot with a valid binding, generates a single mihomo config containing one `proxies` entry per unique node and one `listeners` entry per slot (listening on `127.0.0.1:<port>`), and launches mihomo with that config.
4. **Rebinding** or **enabling/disabling** a slot regenerates the config and hot-reloads it through mihomo's `/configs` endpoint — no process restart required.
5. **Traffic** is sampled from mihomo's `/connections` endpoint and attributed back to each local port and resolved node for the Activity view.

## Runtime Data

User settings, generated configs, and traffic history are stored under `~/.mihomo_switch/` (created on first run). This directory is not part of the repository.

## License

[MIT](./LICENSE)
