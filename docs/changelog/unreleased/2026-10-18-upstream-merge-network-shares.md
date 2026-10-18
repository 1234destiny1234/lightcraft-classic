## Upstream merge

- **Upstream merged (13 commits):** float image samples that are not finite are clipped safely, capture-time shifts
  keep the time zone, search words with an unknown field prefix are plain text, Assisted Culling bursts across
  midnight at month end, offline reverse geocoding at longitude 180, only photo file types reach the system opener,
  and XMP sidecars are written only beside an existing original. (⇠ #2071, ⇠ #2146, ⇠ #2099, ⇠ #2095, ⇠ #2061,
  ⇠ #2135, ⇠ #2175, ⇠ #2199)
- **Network shares keep working:** upstream now follows a Lightroom catalog's originals only on this computer or on
  the share the catalog itself is on. The fork keeps following originals on any network drive or share (mapped
  drives, UNC paths, SMB/NFS mounts) wherever the catalog is; only device paths and URLs are refused. (no PR: fork
  difference)
