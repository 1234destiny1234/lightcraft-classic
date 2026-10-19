## Fixes

- **Mirrored AVIF files display the right way round:** an AVIF whose container mirrors the picture (`imir`) was flipped on the wrong axis, left-right instead of top-bottom and the reverse. It now reads the property as HEIC, libheif and libavif do.
