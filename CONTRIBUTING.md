# Contributing to Swarm

Thank you for your interest in contributing to **Swarm**! Swarm is an open-source, native desktop workspace for autonomous coding agents and multi-agent engineering teams, released under the [Swarm License](LICENSE) (MIT with attribution).

Swarm is built natively with **Tauri 2** (Rust core in `src-tauri/`), vanilla TypeScript (`src/`), and an Astro marketing website (`website/`).

This guide details our local development setup, code quality gates, architecture guidelines for Swarm modules, branch conventions, and Pull Request checklist. Code conventions for developers and AI agents are documented in [AGENTS.md](AGENTS.md).

By participating, you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md). Please report any security vulnerabilities privately through GitHub Security Advisories as described in [SECURITY.md](SECURITY.md).

---

## 🛠️ Prerequisites & System Requirements

- **Node.js**: v22+ and npm.
- **Rust**: Stable toolchain (`rustup update stable`).
- **Git**: v2.35+.
- **Coding CLIs** (Optional for live runs): Claude Code, Codex, Kimi Code, OpenCode, Antigravity, etc.
- **Tauri 2 Prerequisites**:
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`).
  - **Windows**: Microsoft C++ Build Tools ("Desktop development with C++") and the Evergreen WebView2 runtime.
  - **Linux (Debian/Ubuntu / Arch / Fedora)**:
    ```sh
    # Ubuntu / Debian
    sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
      libxdo-dev libssl-dev libdbus-1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
    ```

---

## 💻 Local Setup & Development

```sh
# 1. Clone the repository
git clone https://github.com/HexaDev-De/Swarm.git
cd Swarm

# 2. Install dependencies
npm ci

# 3. Launch in desktop development mode (Tauri 2 + Vite HMR)
npm run desktop        # Equivalent to: npm run tauri dev
```

### Other Helpful Dev Commands
- `npm run dev`: Starts the frontend-only Vite server on `http://127.0.0.1:1420`. When opened in a standard browser, it operates with mock native commands (`src/preview.ts`), which is what Playwright tests use.
- `npm run tauri build`: Compiles the native production executable for your current platform.
- `npm run release:build`: Builds production packages with squashfs cleanup (see [docs/releasing.md](docs/releasing.md)).

---

## 🧪 Quality Verification & Testing

Every Pull Request must pass the complete local verification suite before submission:

```sh
# 1. TypeScript Static Typecheck
npx tsc --noEmit

# 2. Production Bundle Build (creates dist/ for tests)
npm run build

# 3. Node Unit Tests
npm test

# 4. Swarm Feature Unit Tests
node --test tests/ultracode.test.mjs
node --test tests/vibe_suite.test.mjs

# 5. Release Packaging Tests
npm run test:release

# 6. Rust Core Unit Tests
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib

# 7. Strict Rust Clippy Lints
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

---

## 🏛️ Contribution Guidelines for Swarm Modules

When contributing to Swarm's flagship features, please follow these module architecture guidelines:

### 1. ULTRACODE Multi-Agent Engine (`src-tauri/src/ultra_engine.rs`, `src/ultracode.ts`)
- **Semantic Contracts**: Ensure interface contracts are strictly defined and locked before spawning worker subtasks.
- **Worktree Isolation**: Subtask workers must run in isolated Git worktrees under `.git/ultra_worktrees/` to prevent working tree corruption.
- **Cleanup Guards**: Always register subtask directories with `WorktreeCleanupGuard` to ensure cleanup even during unexpected process termination.

### 2. Dual-Harness Arena (`src-tauri/src/arena.rs`, `src/arena.ts`, `src/arena.css`)
- **Telemetry Precision**: TTFT, tokens-per-second, and roundtrip duration must be computed accurately from raw streaming frames.
- **Winner Promotion**: Winning branch promotion must use clean Git worktree merging into `main`.

### 3. Interactive Design Canvas (`src-tauri/src/design_canvas.rs`, `src/design-canvas.ts`, `src/design-canvas.css`)
- **File Storage**: User design artboards are saved strictly in `.swarm/design/` (which is git-ignored). Never commit user artboards to the repo.
- **DOM & AST Sync**: DOM tree mutations must sync cleanly with HTML/CSS properties without introducing invalid CSS syntax.

### 4. Vibe Time-Machine (`src-tauri/src/vibe_time_machine.rs`, `src/vibe-time-machine.ts`)
- **Git-Backed Snapshots**: Turn snapshots must be lightweight git commits/trees tagged with task and turn metadata.
- **Rollback Safety**: Rollbacks must never delete uncommitted user files without confirmation.

---

## 🌿 Branch Naming & Commit Style

### Branch Naming Conventions
- `feat/feature-name` (e.g. `feat/arena-diff-viewer`)
- `fix/bug-description` (e.g. `fix/worktree-cleanup-race`)
- `docs/doc-update` (e.g. `docs/arch-benchmarks`)
- `refactor/subsystem` (e.g. `refactor/ipc-channels`)

### Commit Messages
Use the conventional commit format in the imperative mood:
```
<type>(<scope>): <short description>
```
- **Types**: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`.
- **Examples**:
  - `feat(arena): add real-time TTFT comparison graph`
  - `fix(clippy): resolve redundant closure in slash commands`
  - `docs(readme): add Arch Linux benchmark table`

---

## 📋 Pull Request Checklist

Before submitting your PR, ensure:
- [ ] Your code compiles cleanly with `npx tsc --noEmit`.
- [ ] All unit and release tests pass (`npm test`, `npm run test:release`).
- [ ] Rust tests and clippy pass without warnings (`cargo test --lib`, `cargo clippy --all-targets -- -D warnings`).
- [ ] Zero telemetry, calls home, or external analytics have been added.
- [ ] No API keys, credentials, or personal local paths (e.g., `/home/username`) are hardcoded.
- [ ] Documentation in `README.md` or `docs/` is updated if behavior changed.
- [ ] PR description clearly explains the **what**, **why**, and **how it was tested**.

---

## 📄 Licensing

By contributing to Swarm, you agree that your contributions will be licensed under the [Swarm License](LICENSE).
