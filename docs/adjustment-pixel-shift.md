# Stable adjustment preview placement

## Cause and fix

At Fit, the original draw path fitted each texture using its rounded pixel aspect ratio,
then snapped only textures matching the physical display dimensions. A settled SDR texture,
a smaller draft and an oversampled HDR texture could therefore occupy different rectangles.

The photo's fitted geometry now determines a physical-pixel rectangle once: round its
physical dimensions, then its centred origin. Develop renders use those bounds even when
whole-pixel texture dimensions change slightly. Embedded camera previews retain their own
aspect fitting, including small differences in crop. Existing mismatched-aspect fallbacks,
1:1 positioning, crop meshes and zoom-window placement remain available.

This changes placement only. Preview sizing, adaptive draft policy, render quality, colour
management and HDR scheduling are unchanged. No shader or decoder changes are involved.

## Reproduction and regression coverage

The supplied 3264 × 2448 DNG and JPEG shifted their visible left edge from x=215 at rest to
x=214 while dragging and back to x=215 on release, at a 1600 × 1000 window and 100% scale.
The supplied 6000 × 4000 ARW stayed at x=158. Their different fitted fractional coordinates
explain the different outcome; the fix is not tied to a camera model or file format.

Unit tests cover 4:3, 3:2, odd dimensions, portrait orientation and display scales of 100%,
125%, 150% and 200%, with several draft/final texture sizes. A headless integration test
checks the actual painted photo quad across interaction start and release in SDR and HDR,
and confirms that the texture dimensions really change. Embedded-preview crop preservation
and existing 1:1 / region tests cover compatibility.

## Independent validation

The standalone branch starts at dfd29a82c, before the HDR adjustment-preview commit 29859f267.
Native captures on that baseline with the placement fix show x=215 throughout for the DNG
and JPEG, and x=158 throughout for the ARW, with HDR display output both disabled and enabled.
The baseline's original SDR-during-drag behaviour is deliberately unchanged.

For HDR integration validation, cherry-pick this commit on top of 29859f267 in a detached
checkout and repeat the same checks with HDR display output enabled and disabled.
The source images and native screenshots are local test inputs/artifacts and are not committed.

## Checks on the standalone branch

- The pixel-placement integration test passes all 24 combinations of image dimensions,
  display scale and SDR/HDR output, including a value change during the interaction.
- All 25 detail geometry tests, 51 region tests and six Before/After tests pass. The latter
  now assert the photo's target rectangle instead of refitting it to a cached texture.
- The desktop build and `cargo xtask ci --fast` pass. Android checking is skipped without
  an NDK; dependency-license checking is skipped without cargo-deny.
- Full CI reached the UI suite. Its remaining context-menu failure,
  `headless::tests::click_widget_right_clicks`, also reproduces in an unchanged 29859f267
  checkout before this patch. It is not changed as part of the placement fix.
