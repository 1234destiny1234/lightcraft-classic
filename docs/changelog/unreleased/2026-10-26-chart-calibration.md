## Colour

- **Colour-chart calibration:** `calibrate-chart --shot RAW --corners … [--light …]` reads the 24 patches of an X-Rite ColorChecker Classic (any corner order, perspective allowed), fits a white-preserving forward matrix and the matching colour matrix per light (one or two lights) to X-Rite's 2016 reference values (current or pre-2014 charts) by CIEDE2000-checked Levenberg–Marquardt, and prints a `chart` camera-database entry, with `--dcp`, `--preview` and `--report`. See `docs/chart-calibration.md`. (no PR)
