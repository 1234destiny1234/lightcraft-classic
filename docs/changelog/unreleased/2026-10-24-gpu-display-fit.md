## Performance

- **Display fit on the GPU:** the loupe's Lanczos display fit runs on the device before the readback (`lcc_gpu::try_render_fitted`, within 1 LSB of the CPU's), so a high quality adjustment update renders in ~5 ms instead of ~22 and a drag gets ~58 instead of ~44 updates a second; the histogram is still the whole frame's. Thanks @Kaan88. (no PR)
