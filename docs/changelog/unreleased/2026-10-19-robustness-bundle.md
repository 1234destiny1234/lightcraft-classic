## Fixes

- **Parsers:** XMP drops XML-illegal characters; per-site black levels keep clipped highlights neutral; shared TIFF child IFDs and multi-string ASCII tags are kept; image size maths is checked on wasm32.
- **Rendering:** a zero-width crop no longer makes grain NaN; no colour noise specks in shadows; grain and Texture match between preview and export; float sources aren't treated as clipped in merges; large GPU blurs stay within the driver's time limit; oversized raw images are an error rather than an abort.
- **Network and storage:** truncated downloads are rejected and downloads have an overall deadline; Immich needs https; saving through a symlink keeps the link; the map tile cache and plug-in store write atomically.
- **Commands:** bad `id`/`ids` arguments are errors; `run-action --connect PORT` works; keymap import is all-or-nothing; book export uses the activity stack; extending a large selection is fast.
- **Slideshow:** fit-to-music timing is right with repeat on; serif font detection matches whole words.
