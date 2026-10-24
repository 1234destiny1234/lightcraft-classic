## Performance

- **HDR textures made on the GPU:** an HDR view rendered on the GPU for an HDR display comes back as its `Rgba16Float` texels, made on the device for the window's headroom, instead of float samples the CPU unpacked and encoded (`lcc_gpu::try_render_hdr_texels`): HDR drags shown in HDR 160 → 199 updates a second (high quality: 136 → 171), one update 86 → 76 ms, the slowest interface frame 13–14 → 9 ms. Thanks @Kaan88. (no PR)
