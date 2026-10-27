## Fixes

- **HDR on Wayland at the desktop's brightness:** the window's SDR white was 1.0 of the extended-linear surface,
  which KWin shows at 80 cd/m² as in scRGB, so with HDR on the app was drawn at 80 cd/m² beside a brighter desktop
  (180–280 cd/m² as KDE's SDR brightness is set) and its highlights reached only headroom × 80. It now takes the
  compositor's reference white for the output it reports the headroom of (`wayland_hdr::best_output`,
  `hdr_display::read_compositor_white`) and draws SDR white at `reference / 80`, as Windows does with the user's SDR
  white level; `<PREFIX>_HDR_PAPER_WHITE` still overrides it. Thanks @Kaan88. (no PR)
