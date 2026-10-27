## Performance

- **Canon CRW decodes without an allocation per sample:** the decoder built (and dropped) a "too many samples" error
  string for every sample it wrote; the error is now made only when it happens. All 15 decodable corpus CRWs
  2.86 → 1.19 s (S70 352 → 202 ms, EOS 10D 183 → 49 ms), the same samples. Thanks @Kaan88. (no PR)
