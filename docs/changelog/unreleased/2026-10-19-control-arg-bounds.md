## Fixes

- **Control-channel argument bounds:** `ui.click`/`ui.clickWidget` `count`, `ui.drag`/`ui.dragWidget` `steps` and `ui.render` `size` are now range-checked with a clear error instead of letting one request (e.g. `{"count": 10000000000}`) run the app out of memory.
