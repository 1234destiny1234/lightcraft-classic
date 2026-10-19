## Fixes

- **Place-name download address check:** a custom download address such as `http://127.0.0.1:80@evil.example/` is now refused; the address is parsed and its real host checked instead of matching the text prefix.
