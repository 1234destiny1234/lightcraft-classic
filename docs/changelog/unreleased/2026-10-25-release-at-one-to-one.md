## Performance

- **Releasing a slider when zoomed in:** after an edit at 1:1 or closer, the part on screen renders once more as one full-quality window in the pane's own slot (its stage cache warm from the drag) instead of one render per tile on screen; only the ring of tiles around it is still prepared for a pan. The view is sharp ~70 ms after letting go; with High quality adjustment previews the drag's last update is that window already. Thanks @Kaan88. (no PR)
