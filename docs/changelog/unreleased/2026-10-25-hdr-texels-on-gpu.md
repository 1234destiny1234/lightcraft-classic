## Performance

- **HDR textures made on the GPU:** an HDR view rendered on the GPU for an HDR display comes back as its `Rgba16Float` texels, made on the device for the window's headroom, instead of float samples the CPU unpacked and encoded (`lcc_gpu::try_render_hdr_texels`): one update of an HDR edit shown in HDR 86 → 76 ms, the slowest interface frame in its drags 13–14 → 9 ms. Thanks @Kaan88. (no PR)
