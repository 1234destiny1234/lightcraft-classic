## Display

- **Windows VRR flicker:** replace continuous repainting from fork #216 with an opt-in, restart-required DX12 DirectComposition setting. Keep dialog backgrounds opaque. Adapted from @kmnkv1's original implementation, https://github.com/storytold/photocraft/pull/7035. Physical VRR-monitor verification pending. (upstream https://github.com/storytold/lightcraft/pull/2421)
