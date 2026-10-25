# Run and preview

PandaCode discovers `pandacode-run.sh` in the selected task's checkout, including
its worktree. When present, **Run** appears in the workspace header and beside
attributed changed-file summaries. Discovery reads a bounded file; it never runs it.

Use ordinary POSIX shell commands, keeping development servers in the foreground:

```sh
#!/bin/sh
# pandacode-preview: http://127.0.0.1:3000
set -eu
npm run dev -- --host 127.0.0.1 --port 3000
```

Use commands that actually match the project. The optional `pandacode-preview`
comment can instead name a generated HTML/SVG file, such as `dist/index.html`.
For static files that need no command, use the existing file preview directly.

Run starts a managed **Run** terminal tab in the task's checkout. Repeated clicks
reuse a live tab; an exited tab is replaced. Minimize keeps the process alive;
closing a live Run terminal uses the existing process-close warning. A declared
local URL is checked briefly for readiness, or a generated file is served through
PandaCode's authenticated file-preview server. Switching tasks cancels pending
automatic browser navigation. Click Run again to check a slow server's readiness.

macOS/Linux use `/bin/sh`. Windows uses a native Bash installation, such as Git
Bash, and reports a clear error if none is installed. Script PATH uses the same
installed-tool locations as PandaCode's CLI discovery, including Homebrew. It does
not evaluate shell startup files. Scripts must stay inside the checkout, be UTF-8,
and fit within 64 KiB. No dependency installation is guessed automatically.

Agent instructions ask it to maintain the script when applicable and open a
preview after relevant edits. `pandacode_run_project` and `pandacode_preview_file`
remain registered across resumed turns, with the harness's normal approval policy.
They require the task to be visible and cannot steal another task's browser.
Architect teams leave final launch/preview to the architect. Opening a page is
not reported as successful testing. Harnesses without app-tool transport can
still create the script for the user's Run button.

IPC: `get_run_script({projectId, sessionId?})` returns
`{path, previewUrl?, previewPath?}` or `null`.
`start_run_script_terminal({projectId, sessionId?, cols, rows})` returns a terminal
ID using the existing `terminal-event` stream. `get_run_script_preview` takes the
same workspace context and returns a ready URL or `null`; it does not execute.
