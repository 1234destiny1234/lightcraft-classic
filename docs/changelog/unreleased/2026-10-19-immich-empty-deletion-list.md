## Fixes

- **Immich deletion queue:** `immich.resolveDeletion` with an empty `assets` list now resolves nothing; it had applied every queued deletion. Omit `assets` to resolve all; a non-list value is refused.
