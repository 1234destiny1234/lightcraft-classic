# Calibrating a camera against Lightroom

> **Last updated:** 2026-10-25

The app renders most raws close to Lightroom, but not the same: its camera colour is estimated (from the camera's
own JPEG, spectral data or DNG matrices), not matched. `<cli> calibrate-lightroom` closes that gap one camera model at
a time. You run it on **your own raws** and **Lightroom's exports of the same photos**. It fits the camera's colour
model and one look per Lightroom profile you exported, and writes **one small file of numbers** that you send to the
project. Once it is added, every user of that camera gets the matched colour, and catalogues imported from Lightroom
pick the matching look for each photo's profile.

`<cli>` is the command-line tool shipped with every release (next to the app).

## What you need

- 20 or more raws from **one camera model**, as varied as you can: daylight, shade, **tungsten / indoor light**,
  saturated colours (foliage, sky, skin, flowers, signs). Both warm and cool light matter: the fit places two white
  points, one for each.
- Lightroom Classic (or Lightroom) with those raws in a catalogue.

## Step by step

1. **Write the baseline preset:**

   ```sh
   <cli> calibrate-lightroom preset -o Baseline.xmp
   ```

   It sets every Basic and Presence slider to 0, sharpening and noise reduction (colour too) to 0, turns lens
   profile, chromatic aberration and vignetting corrections off, removes crops and keeps White Balance *As Shot*. It
   picks no profile. Import it in Lightroom's Develop module (Presets ▸ Import Presets…).

2. **Make virtual copies** of the photos (Photo ▸ Create Virtual Copy) so your own edits stay untouched, and apply
   the baseline preset to the copies.

3. **Choose a profile** on the copies (*Adobe Standard* is required; *Adobe Color*, *Camera Standard* and others are
   optional extras, one export each) and **export** with:
   - File format **TIFF**, **16 bits**, colour space **ProPhoto RGB**, no compression or ZIP;
   - image sizing off (full size), output sharpening off;
   - metadata **All** (the tool reads camera model, capture time and the profile name from it);
   - into **one folder per profile**, named after it, e.g. `exports/Adobe Standard/`, `exports/Adobe Color/`.

   Repeat step 3 for each profile you want matched.

4. **Check the set** without fitting:

   ```sh
   <cli> calibrate-lightroom fit --raws ~/Pictures/raws --exports "exports/Adobe Standard" "exports/Adobe Color" --dry-run
   ```

   It prints how many raw ↔ export pairs it found, each file it skipped and why (a different camera, a crop, a
   non-zero slider, no matching raw), the white-balance range covered and the looks found. A narrow range means you
   should add tungsten-lit or daylight photos.

5. **Fit:**

   ```sh
   <cli> calibrate-lightroom fit --raws ~/Pictures/raws --exports "exports/Adobe Standard" "exports/Adobe Color" -o out --contributor "Your Name"
   ```

   Progress goes to the terminal; a fit takes a few minutes. It writes:
   - `out/<Make>_<Model>.json`: **the file to send**;
   - `out/<Make>_<Model>.<look>.dcp`: one camera profile per look, **for your own testing only**. Import it in the
     app (command `camera.importDcp`) to see the result on your photos. Don't send or publish these.

   The tool lists exactly what the JSON holds and the held-out colour error (ΔE00 on photos the fit did not see)
   with the app's current colour and with the calibration. If the calibration doesn't beat the current colour, it
   says what failed (often: add photos under another light).

6. **Send the JSON file**: open an issue or pull request on the project's repository and attach it. `--contributor`
   is optional; leave it out to stay anonymous.

## Privacy: what the file holds

Only aggregate numbers: camera make and model, two colour and two forward matrices, optional hue/saturation maps,
per look an exposure offset, tone curve and optional look table, and the evidence (number of photos and samples,
photos per white-balance range, hue coverage, held-out ΔE00). The optional contributor name and the tool version.

It holds **no image content, file names, dates, serial numbers, locations or lens data**. The file is checked against
an allow-list of fields when written and again when added to the repository: a file with any other field is refused.
Anything that contains your images (the `.dcp` files' test renders, a local report) stays on your computer.

## Why this is allowed (data rules)

Maintainer decision 2026-10-25: a black-box fit to Lightroom's **output images** of a contributor's own photos may
ship, as aggregate statistics under MIT OR Apache-2.0. Adobe's own profile files (DCP, LCP), matrices read from
Adobe-converted DNGs and Adobe-rendered pixels never ship.

## For developers: adding a contribution

```sh
<cli> calibrate-lightroom add path/to/<Make>_<Model>.json [--repo .] [--contributor "Name"]
```

`add` parses and validates the file (format version, allow-listed fields, matrix and table bounds), optionally sets
the contributor, writes the canonical form to `crates/camdb/data/calibrations/<Make>_<Model>.json` and adds (or
replaces) its row in `assets/ATTRIBUTION.md`. Commit both, with the release notes. A file that fails validation is
refused with the reason and nothing is written.

Before committing, try a contribution in a debug build without adding it: point
`<PREFIX>_CAMERA_CALIBRATIONS` at a folder holding the JSON and open that camera's raws. `<cli> calibrate-lightroom
list` prints the calibrations built into a binary, their looks (`cal:<id>`) and their held-out ΔE00.

`<cli> calibrate-lightroom verify --catalog X.lrcat --exports DIR` (whole-pipeline error between the app's render of
an *edited* catalogue and its edited exports) is planned and not available yet.

Sources: own design; DNG specification 1.7 chapter 6 (profile model); CIEDE2000 (Sharma, Wu, Dalal 2005).
