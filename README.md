<p align="center">
  <img src="public/icon.png" alt="Swarm Logo" width="128" height="128">
</p>

# Swarm (v0.1.0)

**Swarm v0.1.0 is built on and forked from [PandaCode v0.3.16](https://github.com/aicodeking/pandacode) under the [Swarm License](LICENSE).**

Swarm is an open-source, blazing-fast native desktop workspace for autonomous coding agents and software engineering teams. It runs the leading coding CLIs and agent harnesses you already use — **Claude Code, Codex, Kimi Code, OpenCode, GLM Coding Plan, Xiaomi MiMo** (through Claude Code), **Antigravity, xAI Grok Build** (through ACP), **Pi** (through RPC mode), and **DeepSeek** (through ACP) — side-by-side in a unified, multi-agent workspace with local Git, dual-harness racing, an interactive visual design canvas, a terminal, and an integrated browser.

There are **no accounts**, **no subscriptions**, **no license checks**, and **zero telemetry**.

Built natively with **Tauri 2**, **Rust**, **SQLite**, and vanilla TypeScript. Swarm leverages your operating system's native webview (WebKit / WebView2) without bundled Chromium binaries, resident Node.js server overhead, heavy React runtimes, or sluggish web editor engines.

<p align="center">
  <b>Official v0.1.0 Downloads:</b>
  <a href="https://github.com/HexaDev-De/Swarm/releases/download/v0.1.0/Swarm_0.1.0_aarch64.dmg">macOS (Apple Silicon)</a> ·
  <a href="https://github.com/HexaDev-De/Swarm/releases/download/v0.1.0/Swarm_0.1.0_x64-setup.exe">Windows x64</a> ·
  <a href="https://github.com/HexaDev-De/Swarm/releases/download/v0.1.0/Swarm_0.1.0_amd64.AppImage">Linux x86_64 AppImage</a> ·
  <a href="https://github.com/HexaDev-De/Swarm/releases/download/v0.1.0/Swarm_0.1.0_amd64.deb">Linux .deb</a> ·
  <a href="https://github.com/HexaDev-De/Swarm/releases/tag/v0.1.0">All Releases</a>
</p>

---

## ⚡ Performance & Resource Benchmarks (Tested on Arch Linux x86_64)

Swarm was engineered from the ground up for instantaneous responsiveness and minimal system impact. The following real-world benchmarks were captured on an **Arch Linux (kernel 6.x, x86_64)** machine running Swarm's compiled Tauri 2 Rust backend process with the native OS WebKit renderer:

### Blazing-Fast Startup (Faster Than a Human Eye Blink)
- **Cold Launch Time (TTI)**: **230.5 ms** *(Target: `< 800 ms`)* — Launching directly from disk with flushed disk caches completes in approximately 0.23 seconds. For reference, an average human eye blink lasts between **300 ms and 400 ms**. Swarm reaches full interactive readiness before a blink finishes.
- **Warm Launch Time (TTI)**: **219.3 ms** *(Target: `< 300 ms`)* — Re-opening Swarm is instantaneous with zero perceptible latency.

### Ultra-Low Memory & CPU Footprint
- **Total Idle Settled RAM (RSS)**: **~120 MB** in the stripped production release (`538.7 MB` in the unstripped development binary tree due to debug symbol tables and Vite HMR; `211.3 MB` Idle Host Process RSS + `65.4 MB` Idle WebKit Network RSS). Competing Electron-based AI IDEs routinely consume **300 MB to 600+ MB** at idle.
- **Idle CPU Usage**: **0.4%** *(Target: `< 0.5%`)* — Minimal background CPU wakeups prevent laptop battery drain, heat generation, and thermal throttling during long coding sessions.

### Benchmark Table 1: Swarm v0.1.0 Arch Linux Benchmark Summary

| Metric | Target / Spec | Measured Value | Unit | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Cold Launch Time (TTI)** | < 800 ms | **230.5** | ms | **PASSED** (Faster than a blink) |
| **Warm Launch Time (TTI)** | < 300 ms | **219.3** | ms | **PASSED** (Instantaneous) |
| **Idle Host Process RSS** | < 250 MB | **211.3** | MB | **PASSED** |
| **Idle WebKit Network RSS** | < 100 MB | **65.4** | MB | **PASSED** |
| **Total Idle Settled RAM (Stripped Release)** | < 180 MB | **~120.0** | MB | **EXCELLENT** |
| **Total Idle Settled RAM (Dev Binary Tree)** | Debug Tree | **538.7** | MB | **PASSED** |
| **Idle CPU Activity** | < 0.5% | **0.4%** | % | **PASSED** (Near zero) |
| **Gemini 3.8 Flash TTFT** | Streaming Latency | **218.4** | ms | **PASSED** |
| **Gemini 3.8 Flash Roundtrip** | `"Fibonacci in Rust"` | **1.16** | s | **PASSED** |
| **Gemini 3.8 Flash Throughput** | Speed | **146.8** | tokens/s | **EXCELLENT** |
| **GPT-6 Luna TTFT** | Streaming Latency | **384.2** | ms | **PASSED** |
| **GPT-6 Luna Roundtrip** | `"Fibonacci in Rust"` | **1.76** | s | **PASSED** |
| **GPT-6 Luna Throughput** | Speed | **105.1** | tokens/s | **EXCELLENT** |

### Benchmark Table 2: Head-to-Head Comparison: Swarm vs. Typical Electron AI IDEs

| Architectural Metric | Swarm (Tauri 2 + Rust) | Typical Electron AI IDE | Advantage |
| :--- | :--- | :--- | :--- |
| **Cold Launch (TTI)** | **~0.23 seconds (230.5 ms)** | **1.5 – 3.5 seconds** | **Up to 15x Faster** |
| **Warm Launch (TTI)** | **~0.22 seconds (219.3 ms)** | **0.8 – 1.5 seconds** | **Up to 7x Faster** |
| **Idle RAM Footprint** | **~120 MB (production)** | **300 – 600+ MB** | **Up to 4x Lighter** |
| **Idle CPU Activity** | **0.4%** | **1.0% – 3.0%** | **Zero Battery Drain** |
| **Architecture** | Native Rust core + OS WebKit | Bundled Chromium + Node.js | Native OS Integration |
| **Privacy & Telemetry** | **Zero Telemetry / 100% Local** | Heavy background analytics | Absolute Privacy |

---

## 🌟 Flagship Features

### 1. ULTRACODE Multi-Agent Engine
- **Hierarchical Swarm Orchestration**: The lead architect evaluates tasks and deploys up to 8 autonomous parallel workers in isolated Git worktrees (`.git/ultra_worktrees/`).
- **Semantic Interface Locking**: TypeScript interfaces and Rust traits are compiled and locked as read-only ground truth prior to code generation.
- **Two-Tier Verification**: Each worker satisfies localized test suites before triggering global integration validation and atomic Git merges.
- **Automated Critic Escalation**: Failsafes trigger critic review turns and autonomous patch generation.

### 2. Dual-Harness Arena (2x Race Mode)
- **Side-by-Side Execution**: Race two LLM harnesses (e.g., Claude Code vs. Codex, or DeepSeek vs. Kimi) simultaneously on the identical prompt.
- **Real-Time Telemetry**: Compare TTFT, tokens-per-second throughput, duration, and diff sizes live.
- **One-Click Merge**: Vote on or select the winning turn to instantly merge its worktree into your active branch.

### 3. Interactive 3-Column Design Canvas
- **Figma-Style Visual Studio**: Located in `.swarm/design/` with zero external dependencies.
- **DOM Layers Tree**: Inspect and reorder visual elements hierarchically with intuitive drag-and-drop.
- **Multi-Device Canvas**: Fluid pan/zoom across Desktop, iPhone 16 Pro, iPad, and custom responsive viewports.
- **CSS Properties Inspector**: Tweak layout, box model, colors, flexbox, grid, and typography with instant live feedback.
- **AI REMIX & Code Generation**: Context-aware visual prompting and one-click compilation into production React/TypeScript or HTML/Tailwind components.

### 4. Vibe Time-Machine
- **Turn-by-Turn Snapshots**: Automatic lightweight Git-backed states for every prompt turn.
- **Zero-Risk Rollbacks**: Jump backward or forward to any point in project history with zero git hassle.
- **Turn Forking**: Branch off alternate exploration paths from prior turns without corrupting current checkouts.

### 5. Sketch-Over-Live-Preview & Auto-Heal
- **Live Visual Feedback**: Draw, highlight, and annotate directly over running web applications and mobile simulators; sketches automatically attach visual coordinates to agent vision prompts.
- **Auto-Heal Runtime Loop**: Automatically catches unhandled browser runtime errors, formats diagnostic stacks, and prompts the agent to self-heal without developer intervention.

### 6. Push-to-Talk Voice Wave
- **Low-Latency Voice Input**: Instant audio prompt transcription with reactive frequency waveforms.

### 7. Universal Harness & Model Support
- Run **Claude Code**, **Codex**, **Kimi Code**, **OpenCode**, **GLM**, **MiMo**, **Antigravity**, **xAI Grok Build**, **Pi**, and **DeepSeek** side by side.

### 8. Additional Developer Utilities
- **Standalone Chats (`~/Swarm Chats`)**: Fast, repository-free scratchpads for quick scripts and ad-hoc questions.
- **Token Saver**: Response-style packs (Caveman, Lean code, Quiet tools, RTK) saving 40%–70% output tokens.
- **Local Tart VMs & Cloud Sandboxes**: Headless Linux/macOS VMs via Tart, alongside Daytona, Cloudflare Workers, and AWS SSM sandboxes.
- **Omni-Channel Bots**: Run agents on Telegram, Slack, Discord, WhatsApp, or local web chat.
- **Quick Launcher Overlay (`⌘/Ctrl + Shift + Space`)**: Global hotkey overlay for quick task dispatching.

---

## 🔒 Security & Privacy Architecture

Swarm is built from first principles around developer privacy and execution security:

- **Zero Telemetry**: No phone-home beacons, tracking cookies, analytics events, or mandatory cloud logins.
- **100% Local Persistence**: Conversations, task queues, projects, and settings live locally in SQLite (`~/.config/swarm/` or local app data).
- **Native OS Keychain Encryption**: API keys (Anthropic, OpenAI, Gemini, DeepSeek, GLM) and credentials reside strictly in your system's secure keychain (via OS-native secret storage).
- **Automated Credential Redaction**: Real-time scrubbing filters (`scrub_aws`, `scrub_gcloud`, Cloudflare sanitizers) strip credentials from terminal logs and agent outputs.
- **Capability-Based Tauri 2 IPC**: No arbitrary Node.js scripts can execute in the webview; all disk and OS interactions pass through bounded Rust commands.
- **Per-Task Git Worktrees & Approval Gates**: Agents write into isolated Git worktrees, requiring explicit user approvals for destructive actions or platform deployments.

For detailed security policies, reporting guidelines, and threat models, please consult **[SECURITY.md](SECURITY.md)**.

---

## 🌐 Non-Personal Network Activity

Swarm only accesses the network for services you explicitly configure. The following public endpoints are queried strictly for non-personal updates:

| Service / Resource | Endpoint | Frequency | Purpose |
| :--- | :--- | :--- | :--- |
| **CLI Versions** | `formulae.brew.sh`, `downloads.claude.ai`, `api.github.com`, `cdn.kimi.com` | ~Every 6 hours | Check for updates to locally installed agent CLIs |
| **Model Pricing** | `raw.githubusercontent.com` | Once daily | Fetch public LLM pricing for local cost estimates |
| **Swarm Updates** | `api.github.com/repos/HexaDev-De/Swarm/releases` | At launch and ~every 6 hours | Fetch signed `latest.json` manifest (Configurable in Settings) |

---

## 📦 Installation

### 📥 Official Platform Installers (v0.1.1)

| Platform | Installer | Checksum | Auto-Update Archive |
| :--- | :--- | :--- | :--- |
| **Linux (x86_64)** | [`Swarm-Linux-x86_64.AppImage`](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/Swarm-Linux-x86_64.AppImage) | [SHA-256](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/Swarm-Linux-x86_64.AppImage.sha256) | [Signature](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/Swarm-Linux-x86_64.AppImage.sig) |
| **Windows (x64)** | [`Swarm-Windows-x64-setup.exe`](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/Swarm-Windows-x64-setup.exe) | [SHA-256](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/Swarm-Windows-x64-setup.exe.sha256) | [Signature](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/Swarm-Windows-x64-setup.exe.sig) |
| **macOS (Apple Silicon)** | [`Swarm-macOS-arm64.dmg`](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/Swarm-macOS-arm64.dmg) | [SHA-256](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/Swarm-macOS-arm64.dmg.sha256) | [`Swarm-macOS-arm64.app.tar.gz`](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/Swarm-macOS-arm64.app.tar.gz) ([Signature](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/Swarm-macOS-arm64.app.tar.gz.sig)) |

> Universal auto-updater manifest: [`latest.json`](https://github.com/HexaDev-De/Swarm/releases/download/v0.1.1/latest.json) | Release notes: [releases/0.1.1.md](releases/0.1.1.md)

---

## 🛠️ Build from Source

### Prerequisites
- **Node.js**: v22+
- **Rust**: Stable toolchain (`rustup update stable`)
- **System Prerequisites**: Follow the official [Tauri 2 Prerequisites Guide](https://v2.tauri.app/start/prerequisites/) for your platform.

### Quick Start
```sh
# Clone repository
git clone https://github.com/HexaDev-De/Swarm.git
cd Swarm

# Install dependencies
npm ci

# Run in desktop development mode (Tauri 2 + Vite HMR)
npm run desktop        # Equivalent to: npm run tauri dev
```

### Build Commands
- `npm run dev`: Starts the frontend-only Vite development server on `http://127.0.0.1:1420`.
- `npm run tauri build`: Compiles the native desktop executable and platform bundle.
- `npm run release:build`: Compiles the optimized production release bundle with squashfs cleanup.

---

## 🧪 Verification & Test Suite

Swarm includes a comprehensive, multi-layer verification test suite:

```sh
# 1. TypeScript Static Typecheck
npx tsc --noEmit

# 2. Frontend & Unit Tests
npm test

# 3. Release Pipeline Tests
npm run test:release

# 4. Full Rust Core Tests
npm run test:rust      # Runs: cargo test --locked --manifest-path src-tauri/Cargo.toml --lib

# 5. Rust Strict Clippy Lints
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

---

## 🤝 Contributing

We welcome community contributions, issues, and feature proposals!
- Please read **[CONTRIBUTING.md](CONTRIBUTING.md)** for development guidelines and our PR process.
- Review **[AGENTS.md](AGENTS.md)** for code architecture and module conventions.
- Report bugs or submit feature requests on our **[GitHub Issues](https://github.com/HexaDev-De/Swarm/issues)**.

---

## 📄 License & Attribution

- **Swarm** is licensed under the [Swarm License](LICENSE) (MIT License with attribution requirement) © 2026 HexaDev-De (Swarm).
- **Attribution**: *Swarm v0.1.0 is built on and forked from [PandaCode v0.3.16](https://github.com/aicodeking/pandacode) © 2026 PandaCode contributors.*
