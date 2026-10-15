# lcc-updater

The app's auto updater (L0, native only): it finds a newer release, downloads the file for this
machine, and stages installing it. There is no update service behind it — the release feed is a file
the release pipeline publishes, plus GitHub's own Releases API as a fallback.

```rust
// what is newer than this build?
let opts = lcc_updater::Options::default();          // brand repository, this version, this platform
if let Some(update) = lcc_updater::check(&opts, None, None)? {
    println!("{} → {}", update.current, update.version);   // 0.7.0 → 0.8.0
    println!("{} ({})", update.asset.name, update.action); // the file, and what installing it does
    // fetch it (resumable, size- and SHA-256-checked) into the cache folder
    let got = lcc_updater::download(&update, &lcc_updater::client()?, dir, None, None)?;
    // say what would happen, then do it (installs over the running installation)
    let plan = lcc_updater::plan(update.asset.kind, &got.path, &update.version.to_text());
    let outcome = lcc_updater::install(update.asset.kind, &got, &update.version.to_text())?;
    if outcome.exit { /* quit: the installer or the replacement script takes over */ }
}
```

- **The feed.** `latest.json` on the newest release (`…/releases/latest/download/latest.json`),
  else `GET /repos/{owner}/{repo}/releases` (no token needed for a public repository; a 403 is the
  rate limit, reported as such), else a URL the caller or `<PREFIX>_UPDATE_URL` names. Both shapes
  parse into `Release`; the newest **version** wins, not the feed's order.
- **Versions** are SemVer 2.0.0 (`v1.2.3`, `1.2.3-rc.1`); a tag that is not a version (`nightly`) is
  not a release of ours. A prerelease is only offered when asked for, and a release outranks its own
  prereleases.
- **The right file** is picked from the artifact names the pipeline already uses
  (`<binary>-<version>-<os>-<arch>…`): the MSI on Windows, the DMG on macOS, the AppImage on Linux
  x86_64/aarch64, and a tarball (or the portable zip) as a manual install anywhere else. An
  installer always wins over an archive; the `SHA256SUMS.txt` asset is read for the hash, never
  installed.
- **Downloads** resume from a `.part` file, are capped, and must match the size the feed states and
  the SHA-256 the release publishes (the size alone when it publishes none) before they are renamed
  into place. A file that fails its check is deleted. A file already there is re-hashed, not trusted
  by name; a hostile asset name (`../../…`) is reduced to its last path segment.
- **Installing** is `install::plan` (what will happen, in words, before anything does) and
  `install::install`: `msiexec` runs the Windows installer; on macOS the disk image is mounted
  read-only, its `<display name>.app` (by that name, never a link) is copied beside the running
  bundle, checked against the release's version and swapped in with two renames that are undone if
  anything fails, then started again once the app has quit (in-process: no Terminal, no script; a
  program that isn't inside an `.app` is pointed to the release page instead); a Linux script
  renames the running AppImage aside and starts the new one, and anything else is revealed in the file manager for a
  manual unpack. Every path ends with the app running again or with a sentence saying what to do.
- **Never a panic.** No network, a rate-limited API, a release without a file for this machine, a
  truncated download, a full disk, an installer that refuses: all are `UpdateError` values a dialog
  can show, and all leave the current installation untouched.

## Sources

Own design, from the documented [GitHub Releases API](https://docs.github.com/rest/releases) and
the layout of release assets; version precedence follows [Semantic Versioning
2.0.0](https://semver.org/spec/v2.0.0.html). The shape of the flow (check → download → install and
restart, with a manifest beside the release) follows Tauri's updater as a design reference, not its
code.

Tests use local HTTP servers for the feed, the checksum list and the file: they make no internet
requests. The end-to-end path (feed → download → staged file) is covered in `lcc-engine`.

The macOS install is after craft-launcher's in-process disk image install
([storytold/craft-launcher](https://github.com/storytold/craft-launcher), Apache-2.0), written anew
from the documented options of `hdiutil(1)`, `ditto(1)`, `plutil(1)` and `open(1)`. Its tests make
their own small disk images (`hdiutil create`) with fake bundles in a temp folder.

## Testing the macOS install by hand

A real install of a real build, from a scratch copy and with no feed involved (never on
`/Applications`, never with the user's settings or library). Build once, package it twice under two
versions, install the newer image over the older bundle. `<version>` is the workspace version
(`Cargo.toml`): the binary is the same in both, so only `Info.plist` tells the two apart, and the
relaunched app reports the workspace version either way.

```sh
export CARGO_TARGET_DIR=$PWD/target/agent-<name> S=$TMPDIR/updater-test
mkdir -p "$S/old" "$S/cfg" "$S/lib" "$S/m"
# no update check in the relaunched app (the test refuses to run without this)
echo '{"version": 1, "updates": {"checkAtStartup": false}}' > "$S/cfg/updates.json"
DIST="$S/a" PACKAGE_VERSION=0.0.1 packaging/macos/package.sh --arch aarch64
DIST="$S/b" PACKAGE_VERSION=<version> packaging/macos/package.sh --arch aarch64 --skip-build
hdiutil attach -readonly -nobrowse -mountpoint "$S/m" "$S"/a/*.dmg
ditto "$S/m/<display name>.app" "$S/old/<display name>.app" && hdiutil detach "$S/m"
UPDATER_TEST_DMG="$(ls "$S"/b/*.dmg)" UPDATER_TEST_BUNDLE="$S/old/<display name>.app" \
  UPDATER_TEST_VERSION=<version> UPDATER_TEST_CONFIG_DIR="$S/cfg" UPDATER_TEST_LIBRARY="$S/lib" \
  cargo test -p lcc-updater --lib a_real_build_is_replaced_in_place -- --ignored --nocapture
```

Afterwards the scratch bundle's `Info.plist` names `<version>`, no `.partial` or `.old` is left
beside it, and the app starts on the scratch settings and library. Then:

```sh
pgrep -fl "$S/old/"                  # the relaunched app runs from the scratch bundle
# look at its window (empty scratch library), then quit it with ⌘Q
pgrep -fl "$S/old/" || echo quit     # nothing runs from it any more
hdiutil info | grep -F "$S" || echo "nothing mounted"
rm -r "$S"
```

The ignored test checks that `$S/cfg/updates.json` turns the start-up check off before it installs anything. The test relaunches with those
added (`open --env <PREFIX>_CONFIG_DIR=… --args --library …`); the app's own relaunch is a plain
`open -n` of the bundle once the app has quit.
