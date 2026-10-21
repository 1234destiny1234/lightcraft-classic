## Develop

- **Lens corrections no longer leave grey corners:** a lens profile, the camera's own distortion correction, manual
  Distortion or Chromatic Aberration now crop just inside the corrected photo, as Lightroom does, so the blank past
  its edge never shows (such as the corner marks on Sony A1 raws). Perspective still keeps its corners unless
  Constrain Crop is on. Fixes fork #245. (no PR)
