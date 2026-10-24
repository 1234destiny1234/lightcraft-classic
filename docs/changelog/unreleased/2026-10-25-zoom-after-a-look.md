## Performance

- **The original prepared for a zoom:** once a photo larger than the preview level has been shown whole for 1.5 s with nothing else waiting, its full-resolution source is decoded in the background (below the neighbours' prefetch), so the first zoom to 1:1 starts from it: ~2.2 → ~1.5 s median until every tile is in. Thanks @Kaan88. (no PR)
