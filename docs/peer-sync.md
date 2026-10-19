# Peer sync

Paired devices on the same network keep a shared library in step, with no server in between. It works
between every platform (macOS, Windows, Linux, Android; iOS later), with any number of devices, and
changes travel across hops: an edit made on device A reaches C through B. This page describes what
exists, its known limits, and the plan for syncing photos and albums.

## What exists

| Part | Where | State |
|---|---|---|
| Discovery: multicast DNS service discovery, `_<svc>._tcp.local`, where `<svc>` is the last segment of `android_package` | `crates/peer/src/beacon.rs`, `dns.rs`, `service.rs` | ✅ |
| Pairing: both screens show a six-digit code, and both people confirm it | `crates/peer/src/secure.rs`, `service.rs` | ✅ |
| Encrypted, mutually authenticated channel (X25519, an Ed25519-signed transcript, HKDF, ChaCha20-Poly1305) | `crates/peer/src/secure.rs` | ✅ |
| Shared library: offered to a paired device, accepted there, then both libraries carry the same sync id | `crates/engine/src/peers.rs`, `<library>/sync.json` | ✅ |
| Catalog exchange: develop settings, rating, flag and colour label of photos both libraries hold | `crates/engine/src/peer_exchange.rs` | ✅ |
| Three-way merge per device pair. When both sides changed a photo, each keeps its own edit and gets the other's as a virtual copy in the same stack | `peer_exchange.rs`, `<library>/sync/<device>.json` | ✅ |
| Settings ▸ Sync: find devices, pair, share the library, answer offers, exchange status | `crates/ui-egui/src/panels/peer_sync.rs` | ✅ |
| `sync.*` commands (UI, CLI, MCP) | `crates/engine/src/cmd/peers.rs` | ✅ |

### How an exchange runs

1. The device with the lower id connects to each paired device it can see, every 15 s.
2. Each side lists the libraries it shares with the other. A library the other side doesn't share yet becomes
   an offer.
3. For a library both share, each side sends one digest per photo. Photos are keyed by the hash of the original
   file's bytes. Virtual copies and deleted photos stay out.
4. Each side asks only for the records it lacks, and they are sent in batches. One side sends while the other
   reads, so a large batch can't stall the exchange.
5. Each side merges on its own UI thread, as undoable steps. It then records what it sent and what it received,
   which is the base for the next three-way merge. A side that kept both versions of a photo tells the other,
   and the other then keeps both versions too.

### Known limits

These are tracked in [gaps.md](gaps.md) as gaps 31–33.

- Not yet tested between real devices. The tests run two or three devices over loopback mDNS on one machine.
- mDNS is IPv4 only and uses one network interface. It has no RFC 6762 probing or conflict resolution, and has
  not been tested against Apple Bonjour or Android NSD.
- Only photos both libraries already hold are matched, so photos, albums and keywords don't travel yet. The plan
  below covers them.
- A collision copy is a virtual copy on the device that made it, and is not sent onward.
- The service name follows `android_package`. Renaming the package changes the service type, so every device
  must be on the new version.

## Plan: syncing photos and albums

The goal is for each device to choose what it syncs (all photos, chosen albums, or picks) and how much of each
photo it keeps (the original, a preview, or settings only). A tablet in the field can then carry previews of
three albums, edit them, and have the edits land on the desktop's originals.

### Principles

- **Settings are resolution-independent**, so an edit made on a preview applies unchanged to the original. This
  already holds for the whole develop pipeline: normalized coordinates and relative radii.
- **Photo identity stays the content hash** of the original. A photo that arrives from a peer, from Immich or
  from a card import is the same photo if the bytes are the same.
- **Choosing what to sync never deletes anything.** Taking an album out of scope stops syncing it. Deleting a
  photo is a separate, explicit change that travels as a tombstone and lands in Recently Deleted on the other
  side.
- **Sync is per library and per device pair**, as now. The scope is chosen per paired device, because the desktop
  may sync everything with the laptop but only two albums with the tablet.
- **Long transfers show in the activity stack**, can be cancelled, and resume from where they stopped.

### Scope: what to sync

Each device chooses, per paired device and shared library:

| Scope | Meaning |
|---|---|
| Everything | Every photo of the library |
| Albums | Chosen albums (including smart albums, whose rules are evaluated on the device that owns the photos) |
| Picks / rating | Photos flagged Pick, or rated at least N stars (combinable with albums) |

A photo is in scope on a device pair if either side's scope includes it. Album membership itself travels for
every album in scope.

### Depth: what each device keeps

Each device chooses this for itself, as a library setting with a per-device override:

| Depth | Keeps | Typical use |
|---|---|---|
| Originals | The original file and its sidecar | Desktop, laptop |
| Previews | A 2560 px preview (JPEG, or AVIF where cheaper) and the metadata. Develops and exports at preview size; the original is fetched when needed | Tablet, phone |
| Settings only | Catalog records, with a thumbnail | Small devices; culling |

A preview-only photo works in Library and Develop like any other photo, with a badge. Export, 1:1 zoom, Merge
and AI denoise ask for the original. It is fetched from any online paired device that has it, or from Immich when
the photo is linked there. The fetch is offered as one step and shows progress in the activity stack.

### Records that travel

The current record (develop settings, rating, flag, colour label) grows into a per-photo record:

- **Metadata:** title, caption, keywords (as paths), GPS and location fields, capture time correction, copyright
  and creator. Each field is merged three-way on its own, so a keyword added on one side and a caption edited on
  the other both survive.
- **Virtual copies:** each copy is keyed by its master's key plus a copy id that is the same on every device.
  Copies, including collision copies, then travel like masters.
- **Versions and history:** versions travel (named snapshots). History stays local, since it is per device and
  grows fast.
- **Stacks:** stack membership and order travel as one value per stack, keyed by the top photo.

### Albums

- Albums get a shared id (random, 128-bit), recorded in a mapping file next to the per-device base
  (`<library>/sync/albums.json`: local id ↔ shared id). The catalog format doesn't change.
- Membership merges three-way as a set: adds and removes from each side both apply. An album renamed on both
  sides keeps the newer name. A deleted album is a tombstone, and its photos stay in the library.
- Album folders (sets) travel as a tree of shared ids.
- Smart albums travel as their rules. Each device evaluates them on its own photos.

### New photos

A photo in scope that the other side lacks:

1. Its record arrives, and a catalog entry is created with a peer source (`RemoteIdentity`, service `peer`,
   account = device id). This reuses the link-only photo support Immich already has. The grid shows the
   thumbnail that came with the record.
2. Depending on the receiving device's depth, the preview or the original is queued. Transfers are chunked
   (1 MiB frames over the existing channel), resume by offset, and are verified against the content hash before
   the file is used.
3. Originals go to `<library>/Originals/Synced/<capture date>/` and previews to the preview cache. On Android
   they go to the app's external files folder, which the Files app can reach.

### Deletions and moves

- **Remove from library:** a tombstone (key plus time) travels, and the other side moves the photo to Recently
  Deleted, where it can be restored. Tombstones are kept for 90 days.
- **Delete from disk** on one device never deletes files on another. It travels as a remove from library.
- **Folder moves and renames** stay local: paths are per device.

### Android and battery

- Transfers run only on unmetered Wi-Fi by default, and pause on low battery (this needs a small Java check on
  connectivity and battery state).
- While the app is in the background, exchanges continue only until Android stops the process. A foreground
  service for long transfers is a later step, and needs the Play policy for data-sync foreground services.

### UI

- Settings ▸ Sync, per paired device: **What to sync** (Everything / Albums… / Picks and rating). **This device
  keeps** (Originals / Previews / Settings only). Storage used by synced files, with **Free up space**, which
  turns originals back into previews where another device has them.
- Library: a badge on preview-only and settings-only photos; **Get Original** in the Photo menu and the
  context menu; **Sync** in an album's context menu (adds it to the scope of every device the library is shared
  with).
- Activity stack: "Syncing photos (n of m)" with bytes, cancellable.

### Stages

Each stage is one or more PRs, in order. Each stage is usable on its own.

| Stage | Content | Depends on |
|---|---|---|
| S1 | Wider records: metadata fields merged one by one, keywords, versions, stacks; virtual copies with shared copy ids | — |
| S2 | Albums: shared ids, set merge, tombstones, folders, smart album rules | S1 |
| S3 | Scope per device pair (Everything / Albums / Picks), Settings ▸ Sync UI | S2 |
| S4 | New photos as peer-sourced entries with thumbnails; previews transferred (chunked, resumable, verified) | S3 |
| S5 | Originals: depth setting, Get Original, fetch from any peer or Immich, Free up space | S4 |
| S6 | Deletions as tombstones into Recently Deleted | S1 |
| S7 | Android: Wi-Fi and battery rules; foreground service for long transfers | S4 |

Hostile-input tests cover every new message and file. A transfer that is cut off, a hash mismatch, a full disk
and a removed peer all end in an error the user can act on, never a panic or a half-written file.

### Decisions for the maintainer

- The default depth on Android: previews, or settings only?
- Whether smart albums sync as rules (proposed) or as their current members.
- Tombstone lifetime (proposed: 90 days).
- Whether a library shared from Immich photos should fetch originals from Immich before asking peers
  (proposed: yes, when the photo is linked).
