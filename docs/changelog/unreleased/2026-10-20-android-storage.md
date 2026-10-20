## Android

- **Folders read in place:** a folder picked on the device's own storage is answered with its real path when the app has "All files access" (`MANAGE_EXTERNAL_STORAGE`, asked once; `app.allFilesAccess` and Settings ▸ Folders ask again), so nothing is copied.
- **Copies in the activity stack:** copies of picked documents (cloud, SD card) report through JNI to an `import` row that can be cancelled; the same document maps to the same staged copy, and staged copies no library photo refers to are deleted at start (only inside `Incoming`, after a day).
- **Extensionless documents:** a picked document whose name has no extension gets one from its MIME type.
