## Formats

- **HEIC in every build:** the `heif` feature (HEIC/HEIF decoding) is now on by default for the desktop app and the CLI, so builds from source (`cargo run`, `cargo xtask run` / `install`) open iPhone photos like the release packages do, instead of failing with "HEIC/HEIF support isn't included in this build". Build with `--no-default-features` to leave HEVC out. (no PR)
