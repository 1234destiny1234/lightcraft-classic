# CI and landing changes

> **Last updated:** 2026-10-13 · **Change:** major (created: PR-only main, self-hosted runner, local status)

## How changes reach `main`

`main` on the fork is protected by two rulesets:

- **No force-push, no deletion** — nobody bypasses this.
- **Pull request required** (0 approvals, merge commits only), the **"Linux (cargo xtask ci)"** check must be
  green and the branch up to date with `main`. Repository admins can bypass it for emergencies only.

Every change goes on a branch, opens a PR and is set to auto-merge
(`gh pr merge <n> --repo <owner>/<repo> --auto --merge`); it lands when the check passes.

## Where the required check runs

The Linux job in `.github/workflows/ci.yml` runs on:

- a **self-hosted runner** (labels `self-hosted, linux, x64, lcc`) for pushes and for PRs from branches in this
  repository: its build cache (`~/.cache/lcc-ci-target`) persists between runs, so most runs are incremental;
- a **GitHub-hosted runner** for PRs from forks, which never run on the self-hosted machine. Outside
  contributors' workflow runs also need a maintainer's approval first.

macOS, Windows, FreeBSD and the other workflows stay on GitHub-hosted runners and don't block merging.

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

When `cargo xtask ci` has to run anyway (or the runner is offline), run it locally on the pushed branch
and post the result as the required check:

```
tools/post-local-ci.sh
```

It refuses with uncommitted changes or an unpushed HEAD, runs `cargo xtask ci`, and posts a commit status
named "Linux (cargo xtask ci)" (success or failure) for HEAD with `gh`. Auto-merge then proceeds. The
status is only as trustworthy as the run behind it: post it only from a real, complete run.
