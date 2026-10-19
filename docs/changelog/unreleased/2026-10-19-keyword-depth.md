## Fixes

- **A keyword with thousands of levels no longer crashes the app:** keywords read from a file now keep their first 128 levels, and the keyword tree shows at most 128, so an `a|a|a|…` keyword can't overflow the stack.
