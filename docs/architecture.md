# Architecture

Three pieces, one direction of dependency:

```
esteban-core  <-  esteban-cli
              <-  app/src-tauri  <-  app/src (React)
```

- `crates/esteban-core`: all launcher logic. Manifests, downloads, Java runtimes, Fabric, Modrinth, the signed channel, auth, profiles, launching. No UI code, no printing.
- `crates/esteban-cli`: a thin console front end. Anything the GUI can do, the CLI can do. It is how launching gets tested without a window.
- `app/src-tauri`: a thin Tauri shell exposing typed commands over `esteban-core`. Capabilities are least-privilege, CSP is strict, nothing is loaded remotely.
- `app/src`: React + TypeScript. Plain CSS with tokens in `styles/tokens.css`, CSS modules per component, fonts bundled locally.

Profiles:

- `clean` gets Fabric, the performance mods and the HUD mod. The hacks artifact cannot be added to it, and a pre-launch guard refuses to start if one is found in its `mods/`.
- `hacks` is opt-in and gets everything in `clean` plus the hacks mod.

## Install and launch

`esteban_core::install::install` does everything a launch needs, in order:

1. Version manifest, then the version JSON, checked against the sha1 the manifest lists.
2. Client jar, libraries (Fabric's win when both list the same artifact), assets, and the Java runtime named by `javaVersion.component`.
3. A brand-new instance (no `options.txt` yet) gets `guiScale:2`, written after a `version:` line with the data version read from the client jar's `version.json`. Without that line the game treats the file as very old and runs every options upgrade on it, and one of those turns off Mojang's accessibility screen for new players. An existing `options.txt` is never touched.
4. Mods from Modrinth (releases only, sha512 checked), plus the hacks jar for hacks profiles only. Picks are written to `mods.lock.json` so a re-run is reproducible. `--update` or a changed mod toggle resolves again. See [Mods](#mods).
5. The clean-profile guard. It opens every jar in `mods/`, including jars nested inside jars, and refuses if the hacks mod is anywhere.

Every download goes to a `.part` file, is checked against its hash and size, and is only then renamed into place. Existing files are re-hashed on each run, so a corrupted file is caught and fetched again.

Launching builds the argument list from the version JSON's rules, adds Fabric's arguments, and starts Java from the bundled runtime.

- **Memory:** heap size comes from system RAM.
- **GC:** on 26.x the GC flags come from Mojang's own `default-user-jvm` block. On 1.21.4 they are G1 with Mojang's tuning.
- **Logging:** the game uses its built-in log4j config. Mojang's separate logging file switches the console to XML events for the official launcher's viewer, which plain-text log streaming does not want.
- **Brand:** `-Dminecraft.launcher.brand=esteban-launcher`. Nothing pretends to be another client.

## Mods

The default set is Fabric API, Sodium, Lithium, FerriteCore, ImmediatelyFast and Iris. Every one except Fabric API can be turned off per instance (`esteban-cli mods disable <mod>`). The choice lives in the instance's `instance.json`.

Resolving, in order:

1. **Pick:** the newest release on Modrinth for the game version and Fabric, with a well-formed sha512. A mod with only beta or alpha builds is skipped and the message says so.
2. **Required dependencies:** followed by project, or by version when Modrinth only names a version. A version pin is treated as "this project is needed", because pins often point at builds for a different game version.
3. **Incompatible on Modrinth:** if a picked mod lists another picked mod as incompatible, the one that declared it is skipped.
4. **Cascade:** a mod whose dependency is missing, turned off or skipped is skipped too, with a message naming the reason.
5. **Jar check:** after download, each jar's `fabric.mod.json` is read (nested jars included, two levels down) and its `depends` and `breaks` are checked against the other jars, the game version, the Java major version and the loader, with Fabric's own version rules. A mod that would stop the game from starting is removed from `mods/` and listed as skipped.
   - Modrinth has no version ranges, so this check is what catches cases like a shader mod built against a newer renderer than the one published for that game version.
   - Unknown ids are left to the game, except `fabric-*` ids, which only Fabric API provides.
   - If any jar's metadata can't be read, missing-dependency checks are switched off for that run, so nothing is skipped by mistake.
   - With `breaks`, the target is skipped, unless the target is Fabric API.

Every skipped mod gets one plain sentence, printed during install and stored in `mods.lock.json`, so it shows up again in `mods list`. If Modrinth can't be reached, the last lock for the same game version and toggles is kept, with a notice. With no lock to fall back on, install stops with a plain message.

Jars the launcher did not put in `mods/` are left alone and listed as "added by you, not checked".

## Pack folders

`esteban-cli packs link --from <game folder>` points an instance's `shaderpacks/`, `resourcepacks/` and `screenshots/` at the same folders in another game folder, for example `~/.minecraft` from Lunar or the vanilla launcher. Nothing is copied. It is a symlink on Linux and a directory junction on Windows, so it needs no admin rights.

- `mods/` is never linked, so another launcher's mods can never reach the clean profile.
- A missing folder or an empty one in the instance is replaced by the link. A folder with files in it is left alone, and the command says to use import instead.
- An existing link is moved to the new source. Only the link changes, never the folder it pointed at.
- A source inside the instance itself is refused.

`packs import` copies the same folders in instead. It never overwrites a file, never writes through a link (a linked folder is reported and skipped), and skips symlinks inside the source. Each file is copied to a `.part` file first and only put in place if nothing with that name exists yet.

`esteban-cli path` prints an instance's folder.

## What gets checked

Every file download carries a hash. The `Download` type has no way to leave it out, and a mismatch is retried and then refused.

| What | Check |
|---|---|
| Version JSON | sha1 from Mojang's version manifest |
| Client jar, libraries, natives | sha1 from the version JSON |
| Asset index, asset objects | sha1 from the version JSON and the index |
| Java runtime manifest and files | sha1 from Mojang's runtime index and manifest |
| Fabric libraries | sha512, sha256 or sha1 from the Fabric profile. Where it gives none (`fabric-loader`, and `intermediary` for 1.21.4), the Maven `.sha256` file next to the jar |
| Modrinth mods | sha512 from the Modrinth version |
| Hacks jar | sha256 pinned in the launcher (signed `versions.json` from M5) |

Not hash-checked, because no source publishes a hash for them: Mojang's version manifest and runtime index, Fabric Meta answers, Modrinth API answers. They are fetched over HTTPS from the [allowed hosts](network.md) only. The Fabric `.sha256` files come from the same host as the jar, so they catch corruption but not a compromised Maven.

Files already on disk are re-hashed on every install and launch, and a mismatch is fetched again.

## Testing a launch

Launching needs a signed-in Microsoft account (M2). There is no offline mode and no way to build one in.

`--smoke-test` waits until the title screen has loaded, keeps the game up for a few seconds, then stops it and exits 0. It fails if the game crashes or takes too long. For a brand-new instance it also writes `onboardAccessibility:false` into the first `options.txt`, so the title screen is reached. Normal launches never touch that setting.
