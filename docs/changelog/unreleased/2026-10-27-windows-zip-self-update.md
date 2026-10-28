## Updater and security

- **Windows zip copies update themselves:** a copy unpacked from the portable zip unpacks the new zip beside the
  program, swaps its files in (renaming the running program aside, which Windows allows) under a locked manifest,
  and starts the new version once the app has ended (it waits on a pipe the old process holds). The previous files
  stay until the new version is up; one that doesn't come up is undone at its next start. Any failure during the
  swap puts the previous files back. Whether the MSI installed a copy is now asked of Windows Installer
  (`MsiEnumClientsW` and `MsiGetComponentPathW` on the app's component, through the new `lcc-winmsi` crate), so a
  zip copy without `portable.txt` no longer gets the MSI; a copy under Program Files is never swapped. (no PR)
