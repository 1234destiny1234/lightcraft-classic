## Performance

- **The original prepared after a look:** once a photo larger than the preview level has been shown whole for 1.5 s with nothing else to render, its full-resolution source is decoded in the background (below the neighbours' prefetch), so the first zoom to 1:1 starts from it: median until every tile is in 2.27 → 1.69 s on 8 raws. A 100 MP raw can still be decoding at the zoom, so the gain varies there. Thanks @Kaan88. (no PR)
