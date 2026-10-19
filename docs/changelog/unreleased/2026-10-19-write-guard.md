## Fixes

- **Keymap and book exports never overwrite originals:** `app.keymapExport` and Export Book now refuse a path that is a catalogued photo's original or sidecar, like the other exports (reachable from MCP or the control channel).
