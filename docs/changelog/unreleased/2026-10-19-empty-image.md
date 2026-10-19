## Fixes

- **A malformed image claiming zero width or height no longer crashes:** such files are now refused as malformed when decoded, and an empty image reaching sampling code reads as black instead of panicking.
