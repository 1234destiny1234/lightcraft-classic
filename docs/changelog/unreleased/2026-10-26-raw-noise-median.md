## Performance

- **Hot-pixel noise model by median selection:** each brightness bin of the noise model (thousands of samples) took
  its median by sorting the whole bin; it now selects it (`select_nth_unstable_by`, the lower half's maximum for an
  even count), bit for bit the same median. The hot-pixel step: A7 III 35 → 30 ms, GFX100S 197 → 167 ms, 105 MP
  Hasselblad 155 → 132 ms, the same repaired samples. `examples/open_bench` times decoding and the step apart.
  Thanks @Kaan88. (no PR)
