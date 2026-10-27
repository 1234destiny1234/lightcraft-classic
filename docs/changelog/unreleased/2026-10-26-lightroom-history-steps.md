## Import

- **Lightroom history from Lightroom Classic 13 and later:** catalog import reads history steps (and snapshots)
  stored compressed, so they come in as History and Versions instead of being skipped with a warning, and are kept
  in the recovery archive. Decoding is capped at 32 MiB per table; an archive too big with the history settings is
  saved without them instead of not at all. Ported from upstream PR 2838.
