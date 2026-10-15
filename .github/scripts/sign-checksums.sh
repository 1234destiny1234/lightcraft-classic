#!/usr/bin/env bash
# Signs a release's SHA256SUMS.txt for the in-app updater, which installs nothing without a valid
# signature (crates/updater/src/signature.rs, docs/releasing.md → Update signing key).
#
#   UPDATE_SIGNING_KEY=<private key file contents> .github/scripts/sign-checksums.sh <folder>
#
# Writes <folder>/SHA256SUMS.txt.sig, then checks it against the public key in brand.toml
# (`$BRAND_FILE` if set): a key that doesn't match fails the step, before anything is uploaded.
# With no key it writes no .sig and warns, like the other signing secrets: the release still
# builds, and the app links to it instead of installing it.
# Used by .github/workflows/release.yml (draft release job); runnable locally the same way.
set -euo pipefail
dir="${1:?usage: sign-checksums.sh <folder holding SHA256SUMS.txt>}"
[ -s "$dir/SHA256SUMS.txt" ] || { echo "::error::$dir/SHA256SUMS.txt is missing or empty"; exit 1; }
rm -f "$dir/SHA256SUMS.txt.sig"
if [ -z "${UPDATE_SIGNING_KEY:-}" ]; then
  echo "::warning::UPDATE_SIGNING_KEY is not set: SHA256SUMS.txt is not signed, and the app will not install this release by itself (it links to the release page)"
  exit 0
fi
# the key reaches xtask through the environment only, never the command line or the log
cargo xtask update-key sign "$dir"
if ! cargo xtask update-key verify "$dir"; then
  rm -f "$dir/SHA256SUMS.txt.sig"
  echo "::error::SHA256SUMS.txt.sig does not verify against brand.toml's [updates] public_keys (the error above says why): nothing is uploaded"
  exit 1
fi
echo "SHA256SUMS.txt.sig verified"
