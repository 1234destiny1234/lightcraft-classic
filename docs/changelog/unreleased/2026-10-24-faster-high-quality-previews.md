## Performance

- **Smoother high quality adjustment previews:** the Lanczos display fit of every settled loupe, Before and hover render reads and writes bytes directly (~30 → ~7 ms at 2560 px, same pixels), so a high quality drag gets ~46 instead of ~28 updates a second. With the setting on, the Histogram panel shows each update's own histogram (HDR bins included), so letting go of the slider no longer changes it; after a drag shown in HDR it keeps the live histogram until the released view renders, instead of flashing the one from before the drag. Thanks @Kaan88. (no PR)
