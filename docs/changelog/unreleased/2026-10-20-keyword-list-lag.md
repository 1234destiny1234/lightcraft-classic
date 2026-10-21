## Fixes

- **Keywording lag with big keyword lists:** with thousands of listed keywords (e.g. an imported Lightroom Classic keyword file), the Keywording panel's suggestions and the Add keyword box's offers rescanned the whole list on every selection change and keystroke (over 100 ms each). The catalog now keeps a keyword index, and both take well under a millisecond (`cargo xtask bench-keywords`).
