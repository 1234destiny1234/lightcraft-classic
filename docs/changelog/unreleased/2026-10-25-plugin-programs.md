## Library

- **Plug-ins start installed programs:** a `programs` permission, granted only by ticking it in the Plug-in Manager, lets a plug-in start a program on the computer with files in a folder of its own and read what the program left there when it ends (`program.list`, `program.start`, the `program.end` hook, a row in the activity stack). macOS finds programs by bundle id, Windows by App Paths and `PATH`, Linux by `PATH`. (no PR)
