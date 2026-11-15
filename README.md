# Meteor Client for Horizon

A port of [Meteor Client](https://github.com/MeteorDevelopment/meteor-client) (the
Minecraft Java utility client) to [Horizon](https://github.com/horizon-rust/horizon)
(Minecraft: Java Edition 26.3 rewritten in Rust), as a **native Horizon trait mod**.

This repository is the **buildable bundle** - the layout
`horizon-launcher native-build` and the launcher's "Add from GitHub" both expect:

```
horizon-mixin.toml     [mod] id = "meteor"; [compatibility] horizon_revision; [api] path = "engine"
engine/                the [api] package: hooks() + the whole port (src/lib.rs, mod/, shim/)
core/                  the engine-agnostic systems, registered as the [[crate]] "core"
```

It is generated from the port's workspace by `tools/pack_bundle.py`, so the sources
here are a byte-for-byte copy of the port's crates: `engine/mod/` is
`crates/meteor-mod/src`, `core/` is `crates/meteor-core`.

## Install

* **From GitHub (launcher):** Mods -> "Add from GitHub" -> this repository's URL
  (or `/tree/main`, or `/commit/<sha>` to pin) -> trust the native code -> build.
* **From a folder:** `horizon-launcher native-build <this folder> --trust-native-code`.
* **From a release asset:** the `.hnmod` asset of a release installs the same bundle.

The build pins Horizon commit `ba6344a59cb9e755b730dcf02285fc0d33b97d3c`
(Nightly 2026-10-30). A build for another Nightly needs that commit in
`horizon-mixin.toml`'s `[compatibility] horizon_revision`, or the launcher's
"Build anyway".

## Use

* **Right Shift** opens the ClickGUI: left click toggles a module, right click opens
  its settings, `H` toggles the HUD editor, Escape closes.
* Commands use the `.` prefix: `.toggle kill-aura`, `.toggle all on`, `.toggle hud`,
  `.setting kill-aura range 4.5`, `.bind kill-aura`, `.friend add <name>`, `.save`,
  `.reload`, `.help`.
* Config is written as SNBT (`modules.nbt`, `hud.nbt`, `friends.nbt`, `config.nbt`) in
  the mod's data folder; the client loads it on start and `.save` writes it.

## Contents at a glance

* 169 modules, the same names, titles, descriptions, categories and setting tables
  as Meteor's `Modules.java` (Combat 29, Player 31, Movement 31, Render 38,
  World 24, Misc 16).
* The systems: module manager and keys, settings (9 kinds plus list settings),
  SNBT config, friends, the HUD model, the ClickGUI, 25 commands, the Meteor theme.
* Where Horizon's mod API cannot express a Java behaviour, the source carries a
  `// PORT GAP:` comment at that spot.

## License

GPL-3.0-or-later, as Meteor Client. See `LICENSE`.
