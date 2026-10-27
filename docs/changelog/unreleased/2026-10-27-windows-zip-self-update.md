## Updater and security

- **Windows zip copies update themselves:** a copy unpacked from the portable zip unpacks the new zip beside the
  program, swaps its files in (renaming the running program aside, which Windows allows) and restarts; any failure
  puts the previous files back. Whether the MSI installed a copy is now asked of Windows Installer
  (`MsiLocateComponentW` on the app's component, through the new `lcc-winmsi` crate), so a zip copy without
  `portable.txt` no longer gets the MSI. (no PR)
