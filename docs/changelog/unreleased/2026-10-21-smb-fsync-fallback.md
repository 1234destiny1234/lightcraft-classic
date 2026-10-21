## Fixes

- **Libraries and sidecars on an SMB share (macOS):** the catalog, XMP sidecars, smart previews and DNG conversions are written again on network shares that refuse macOS's full flush; the write falls back to a plain `fsync` instead of failing.
