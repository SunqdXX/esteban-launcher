# Third-party

Nothing below is bundled in our releases or this repo, except the fonts. Mods are downloaded unmodified from Modrinth on the user's machine at install time and checked against the hashes Modrinth publishes.

## Default mods

| Mod | License |
|---|---|
| Fabric API | Apache-2.0 |
| Sodium | PolyForm Shield 1.0.0 |
| Iris | LGPL-3.0-only |
| Lithium | LGPL-3.0-only |
| FerriteCore | MIT |
| ImmediatelyFast | LGPL-3.0-or-later |

## Loaders

| Loader | License | Notes |
|---|---|---|
| Fabric Loader | Apache-2.0 | |
| Minecraft Forge | LGPL-2.1 | Downloaded from Forge's maven like every third-party launcher does. Forge is supported by people visiting [minecraftforge.net](https://minecraftforge.net/). |

## Not in the defaults

**Entity Culling** is licensed under the tr7zw Protective License, which says:

> The Software may not be used to get a) a commercial advantage, or b) monetary compensation.

The launcher has a donation page, so Entity Culling is left out of the defaults.

## Fonts (bundled)

| Font | License | Notes |
|---|---|---|
| IBM Plex Sans | SIL Open Font License 1.1 | Reserved Font Name "Plex". Shipped unmodified. `app/src/assets/fonts/plex-sans/OFL.txt` |
| Departure Mono | SIL Open Font License 1.1 | `app/src/assets/fonts/departure-mono/OFL.txt` |

## Code dependencies

Rust crate licenses and advisories are checked by `cargo-deny` in CI (`deny.toml`). npm packages are dev tooling plus React and the Tauri API, all MIT or Apache-2.0.
