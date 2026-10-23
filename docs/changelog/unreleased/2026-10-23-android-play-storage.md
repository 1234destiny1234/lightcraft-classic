## Release builds and CI

- **Play Store build:** `cargo xtask android package --aab` now builds the Play variant, whose manifest has no `MANAGE_EXTERNAL_STORAGE` or `requestLegacyExternalStorage` (Play refused the bundle); the GitHub APKs keep All files access for reading folders in place (`--store github|play`). The app never asks for a permission its manifest lacks, the camera roll handles Android 14's selected-photos access, and raws MediaStore doesn't count as images are listed where All files access allows. (no PR)
