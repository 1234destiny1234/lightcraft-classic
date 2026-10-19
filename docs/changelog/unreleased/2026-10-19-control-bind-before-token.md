## Fixes

- **Second instance on the same control port:** an app started with a `--control` port that another instance already serves no longer replaces that instance's token file before failing to bind, so MCP and CLI clients keep reaching the first instance; the token file is now written atomically.
