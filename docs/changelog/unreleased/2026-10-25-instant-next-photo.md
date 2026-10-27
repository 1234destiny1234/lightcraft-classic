## Performance

- **The next photo is sharp at once:** the Detail view prepares its neighbours with the loupe's own render job and adopts the result when you step to one: the adjacent photo is ready 237 → 85 ms (median of 14 filmstrip steps). A jump to a photo further away still renders behind a stand-in. Thanks @Kaan88. (no PR)
