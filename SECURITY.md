# Security & Privacy Policy: Swarm

Swarm is committed to delivering a private, secure, and transparent engineering workspace for autonomous coding agents. This document describes our security guarantees, threat model, credential management, execution sandboxing, and reporting process.

---

## 1. Supported Versions

Security patches and vulnerability remediations are actively released for the following versions:

| Version | Supported | Release Status |
| :--- | :--- | :--- |
| **0.1.x** | **Yes** | **Active Stable Release (Current)** |
| < 0.1.0 | No | End of Life (Upgrade to v0.1.0+) |

---

## 2. Reporting a Vulnerability

If you discover a security vulnerability or sensitive information leakage within Swarm, **please do not open a public issue.**

To report vulnerabilities responsibly:
1. Navigate to the repository's GitHub Security Advisory portal:
   👉 **[https://github.com/HexaDev-De/Swarm/security/advisories](https://github.com/HexaDev-De/Swarm/security/advisories)**
2. Click **"Report a vulnerability"** to submit your findings privately to the maintainers (`HexaDev-De`).
3. Alternatively, contact the maintainers directly via GitHub private disclosure.

### What to Include in Your Report
- The Swarm version, operating system, and architecture (accessible via `Settings -> About -> App details for support`).
- A clear description of the potential impact, attack vector, and severity.
- Step-by-step reproduction instructions or a minimal proof of concept (PoC).
- Relevant sanitised terminal logs or screenshots (ensure all API keys, personal paths, and tokens are redacted).

### Response Timeline
- **Initial Acknowledgment**: Within 48 hours of receipt.
- **Triage & Status Updates**: Within 5 business days with progress on remediation.
- **Coordinated Disclosure**: Fixes will be prepared, verified, and released with an advisory crediting the reporter (unless you request anonymity).

---

## 3. Zero-Telemetry & Local-First Privacy Guarantees

Swarm is built from first principles around strict developer data sovereignty:

- **No Remote Telemetry**: Swarm contains zero tracking cookies, analytics SDKs, session replay trackers, or crash-reporting beacons.
- **No Intermediary Cloud Relay**: Prompts, codebase files, diffs, and chat transcripts are never routed through Swarm-owned servers. All network calls go directly to the LLM provider API or local agent CLI you configure.
- **Local SQLite Storage**: Tasks, projects, conversation sessions, and configuration are persisted strictly on your local disk in a SQLite database (`~/.config/swarm/` on Linux, `~/Library/Application Support/com.hexadev.swarm/` on macOS, and `%APPDATA%\com.hexadev.swarm\` on Windows).
- **Zero Mandatory Accounts**: You do not need to register an account, sign in, or provide personal details to use any feature in Swarm.

---

## 4. Keychain & Credential Scrubbing Architecture

### Native OS Keychain Storage
Swarm never stores plaintext API keys or authorization secrets on disk:
- Third-party provider credentials (Anthropic, OpenAI, Gemini, DeepSeek, GLM, MiMo, Cloudflare, Daytona) and bot channel tokens (Telegram, Discord, Slack, WhatsApp) are stored encrypted in the OS-native credential manager:
  - **macOS**: Keychain Services via Security.framework.
  - **Windows**: Windows Credential Manager via CryptProtectData.
  - **Linux**: Secret Service API (Freedesktop secret storage) via DBus / libsecret.

### Automated Credential Redaction Pipeline
Swarm employs aggressive automated scrubbing filters on all terminal buffers, process logs, and agent transcripts before data touches the UI or persistent storage:
- `scrub_aws`: Strips AWS Access Key IDs (`AKIA...`), Secret Keys, and session tokens.
- `scrub_gcloud`: Redacts Google OAuth tokens, service account keys, and private certificates.
- `scrub_cloudflare`: Removes Cloudflare API tokens, Global Keys, and Zone IDs.
- **Generic Token Sanitizers**: Automatic regex masking for Anthropic (`sk-ant-...`), OpenAI (`sk-...`), GitHub tokens (`ghp_...`, `github_pat_...`), Slack tokens (`xoxb-...`), and generic private keys (`-----BEGIN PRIVATE KEY-----`).

---

## 5. Execution Isolation & Sandboxing

### Capability-Based Tauri 2 IPC
- Swarm does not bundle an unrestricted Node.js runtime inside the webview renderer.
- The webview runs under a strict Content Security Policy (CSP) with `'wasm-unsafe-eval'` permitted solely for the vendored Enriched Markdown parser.
- All file system operations, process invocations, and git commands must pass across a strongly-typed, capability-gated Tauri 2 IPC boundary implemented in Rust.

### Per-Task Git Worktree Isolation
- Agents perform work inside isolated Git worktrees rather than polluting your primary working directory.
- Diffs, staging, and commits remain segregated until you explicitly click **Merge into main** or approve the completed turn.
- In ULTRACODE mode, up to 8 subtask workers run in dedicated sandboxed worktrees (`.git/ultra_worktrees/`), preventing race conditions and unexpected file overwrites.

### Virtual Machine & Cloud Sandboxes
- **Apple Silicon VMs**: Hardware-accelerated headless macOS and Linux VMs driven via Tart (`src-tauri/src/vm*`) on `Virtualization.framework`, completely isolating untrusted scripts.
- **Cloud Containers**: On-demand sandbox execution via Daytona REST APIs, Cloudflare Sandbox Container Workers, and AWS SSM-isolated EC2 instances.

### Human-in-the-Loop Approval Gates
- Destructive commands, file modifications outside the active checkout, and cloud deployment CLIs (`pandacode_cloud_cli` for Vercel, Wrangler, Railway, AWS, and GCP) are blocked by default behind an explicit visual approval bar.
- Automated executions only proceed when you explicitly toggle **Full access** or approve a specific pending action.

---

## 6. Scope

### In Scope
- Swarm native desktop application binaries (macOS, Windows, Linux).
- Rust core architecture (`src-tauri/src/`), Tauri 2 IPC boundaries, and database storage.
- Plain TypeScript frontend, UI components, and state controllers.
- Local bots server (`bots/channels/http.rs`) binding to `127.0.0.1`.
- GitHub Actions release build workflows and automated artifact signing pipelines.

### Out of Scope
- Third-party coding CLIs (Claude Code, Codex, Kimi, OpenCode) executed on the host system.
- External API providers (Anthropic, OpenAI, Google, DeepSeek) and their upstream infrastructure.
- User-directed destructive commands executed after explicit user approval.
