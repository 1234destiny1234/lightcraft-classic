## Fixes

- **An older release no longer drops your newer edits:** opening a library made by an older release now marks its catalog with the new format straight away, so the older release refuses it instead of opening it and silently losing fields it doesn't know (such as a cleared GPS position). If the catalog can't be marked (a full disk, a catalog that can't be written), the library isn't opened and is left unchanged.
