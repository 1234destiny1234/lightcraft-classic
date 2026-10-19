## Fixes

- **Header injection in plugin requests:** a content type or header value containing CR/LF or other control characters is now refused instead of being written into the HTTP request, so a plugin can no longer smuggle extra headers.
