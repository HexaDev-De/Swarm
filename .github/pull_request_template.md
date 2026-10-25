## What and why

<!-- What does this change, and why? Link the issue: Fixes #123 -->

## How it was tested

<!-- Commands you ran and what you checked by hand. Add screenshots for UI changes. -->

- [ ] `npm run build` (typecheck + bundle)
- [ ] `npm test`
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` (if Rust changed)
- [ ] Relevant Playwright tests in Chrome and WebKit (if UI changed)

## Checklist

- [ ] Commit messages follow `<type>: <description>`
- [ ] Docs in `docs/` updated if behavior changed
- [ ] Preview mocks added in `src/preview.ts` for new IPC commands
- [ ] No telemetry, secrets or personal paths added
