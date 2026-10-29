## Import, export and formats

- **Lightroom Classic export preset import:** `.lrtemplate` export presets whose settings use unprefixed keys (`format`, `jpeg_quality`, `size_doConstrain`, `outputSharpeningOn`, `embeddedMetadataOption`, `renamingTokensOn`, `tokens`, `collisionHandling`, `useWatermark`, etc.) or carry a dormant 16-bit depth alongside JPEG now keep their supported settings, populate the Export dialog on import, and list any unsupported settings in both the import toast and the Export dialog. (no PR)
