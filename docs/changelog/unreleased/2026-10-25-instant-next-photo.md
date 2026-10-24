## Performance

- **Instant next photo:** the Detail view's neighbour prefetch renders the very job the loupe asks for once that photo is shown (the preview-level render and its display fit), and the loupe adopts it on the step instead of rendering again: after a short look at a photo, stepping to the next is sharp in ~33 ms (was ~85 ms median, up to ~350 ms, with a stand-in first). Thanks @Kaan88. (no PR)
