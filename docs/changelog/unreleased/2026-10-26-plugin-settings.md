## Interface

- **Plug-in settings:** plug-ins can declare settings, kept for the whole app and edited on the plug-in's page (a *Settings…* button in the Plug-in Manager; `plugin.settings`, `plugin.setSettings`). Command and export-destination dialogs open with the values they last ran with (`remember: false` opts a field out), and a command with `prepare: true` is asked first how to run on the selection: it offers its own options, runs at once, or explains why there is nothing to do (`plugin.prepare`). Host feature `settings`; new example `sdk/examples/keyword-picker`. (no PR)
