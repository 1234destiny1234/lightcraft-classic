## Fixes

- **Thumbnail render loop:** two views showing one photo at different sizes, such as the Slideshow or Print preview next to the Navigator, no longer replace each other's thumbnail render on every frame. The larger render now serves both, so those modules stop using the CPU while idle.
