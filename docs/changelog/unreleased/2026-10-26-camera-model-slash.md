## Fixes

- **Camera model slashes:** `calibrate-lightroom fit` and `add` accept camera and look identifiers containing slashes (e.g. phone models such as `Xiaomi 17 Ultra/2512BPNDAG` and film profiles), sanitising filenames and profile slugs while preserving the exact model identifier. Fork #542. (no PR)
