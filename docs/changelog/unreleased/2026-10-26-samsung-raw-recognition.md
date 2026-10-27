## Fixes

- **Samsung NX1 / NX500 / NX3000 / NX mini raws are raws again:** the file sniffer counts TIFF compression 32772 or
  32773 as raw data only with a Samsung Make and a CFA pattern in the same directory (IFD0 or a SubIFD), so these
  SRWs route to the raw decoder while an ordinary PackBits TIFF stays a TIFF. Thanks @Kaan88. (no PR)
