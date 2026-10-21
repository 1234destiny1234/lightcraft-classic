## Performance

- **GPU histogram:** GPU renders bin the histogram, vectorscope and waveform grids on the GPU and read them back with the image, removing the CPU pass (24 MP render 75 → 68 ms); the CPU path remains the oracle. Milestone M5 is complete.
