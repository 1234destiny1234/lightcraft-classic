## Fixes

- **SFTP host keys are confirmed, not trusted on first use:** web gallery upload and SFTP publishing refuse a server whose host key is not pinned yet, before any password or key is sent; the Web module's Check Host Key shows the fingerprint to confirm (`web.hostKey`, `web.trustServer`), and a changed key still aborts before login.
