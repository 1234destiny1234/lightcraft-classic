# CI and landing changes

> **Last updated:** 2026-10-13 · **Change:** major (fast PR gate; full test runs only before a release); major (created: PR-only main, self-hosted runner, local status)

## How changes reach `main`

`main` on the fork is protected by two rulesets:

- **No force-push, no deletion** — nobody bypasses this.
- **Pull request required** (0 approvals, merge commits only), the **"Linux (cargo xtask ci)"** check must be
  green and the branch up to date with `main`. Repository admins can bypass it for emergencies only.

Every change goes on a branch, opens a PR and is set to auto-merge
(`gh pr merge <n> --repo <owner>/<repo> --auto --merge`); it lands when the check passes.

## Fast gate on PRs, full CI before a release

| Trigger | Linux ("Linux (cargo xtask ci)") | macOS / Windows |
|---|---|---|
| Pull request to `main`, push to `main` | `cargo xtask ci --fast` + SIL suite | build + clippy |
| Push to `release`, manual dispatch (Actions → CI → Run workflow) | full `cargo xtask ci` + SIL suite | build + clippy + `cargo test --workspace` |

`cargo xtask ci --fast` runs fmt, brand, docs, clippy `-D warnings` (incl. the HEIF feature), parity refs,
shared-check, layers, assets, deny and the wasm check; it skips the workspace and HEIF test runs. The SIL step
runs `cargo xtask sil` (docs/testing-sil.md) once that command exists and is skipped until then. Locally,
`cargo xtask ci` stays the full run; use `--fast` per commit and run the targeted tests of what you touched.

Before cutting a release, dispatch CI on the commit (or push it to `release`, which also starts
`release.yml`) and make sure all three OSes are green.

## Where the required check runs

The Linux job in `.github/workflows/ci.yml` runs on:

- a **self-hosted runner** (labels `self-hosted, linux, x64, lcc`) for pushes and for PRs from branches in this
  repository: its build cache persists between runs, so most runs are incremental. Each runner instance has
  its own cache (`CARGO_TARGET_DIR` from the runner's `.env`, else `~/.cache/lcc-ci-target-<runner name>`), so
  several instances on one machine (`gpu0` and `gpu0-b`) can run jobs at once without fighting over one
  build directory;
- a **GitHub-hosted runner** for PRs from forks, which never run on the self-hosted machine. Outside
  contributors' workflow runs also need a maintainer's approval first.

The macOS and Windows "clippy + build" jobs run the same way on self-hosted runners (labels `self-hosted,
macOS, ARM64, lcc` and `self-hosted, windows, x64, lcc`). FreeBSD and the other workflows stay on
GitHub-hosted runners. Only the Linux check blocks merging.

### macOS runner notes

The runner is a LaunchAgent of the logged-in user (`./svc.sh install` in `~/actions-runner`, no sudo), using
that user's `rustup` in `~/.cargo/bin` (set in the runner's `.path`) and Xcode's toolchain. `.env` sets
`CARGO_TARGET_DIR=~/.cache/lcc-ci-target`. A LaunchAgent runs only while that user is logged in.

### Windows runner notes

The runner runs as a service under NETWORK SERVICE, so Rust is installed machine-wide
(`RUSTUP_HOME=C:\rust\rustup`, `CARGO_HOME=C:\rust\cargo`, with `C:\rust\cargo\bin` on the machine
`Path`) and NETWORK SERVICE may modify `C:\rust`. The runner's `.env` file sets those two variables and
`CARGO_TARGET_DIR=C:\lcc-ci-target` (the persistent build cache); its `.path` file holds the machine
`Path`. Visual Studio Build Tools (C++) provide the linker. The `.path` file puts
`C:\Program Files\Git\bin` first, so workflow `bash` steps get Git Bash rather than the WSL launcher in
`System32` (which fails under the service account).

### Setting up the runner machine

1. Install the system libraries the build needs (Wayland, X11, xkbcommon, OpenGL, udev development packages;
   on Debian/Ubuntu the list is in the workflow's "Install system libraries" step), `git`, a C linker, and
   `rustup` (the workflow installs the stable toolchain, `wasm32-unknown-unknown`, rustfmt and clippy).
2. Repository → Settings → Actions → Runners → **New self-hosted runner** (Linux x64); follow the commands,
   and when `config.sh` asks for labels add **`lcc`**.
3. Run it as a service (`sudo ./svc.sh install && sudo ./svc.sh start`) under a normal user, not root.
4. Settings → Actions → General → *Fork pull request workflows from outside collaborators*:
   **Require approval for all outside collaborators**.

If the runner is offline, jobs for our own branches wait in the queue; post a local result instead (below).

## Posting a local result

When `cargo xtask ci` (the full run) has to run anyway (or the runner is offline), run it locally on the pushed branch
and post the result as the required check:

```
tools/post-local-ci.sh
```

It refuses with uncommitted changes or an unpushed HEAD, runs `cargo xtask ci`, and posts a commit status
named "Linux (cargo xtask ci)" (success or failure) for HEAD with `gh`. Auto-merge then proceeds. The
status is only as trustworthy as the run behind it: post it only from a real, complete run.
