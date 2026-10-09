# Architecture

Three pieces, one direction of dependency:

```
esteban-core  <-  esteban-cli
              <-  app/src-tauri  <-  app/src (React)
```

- `crates/esteban-core`: all launcher logic. Manifests, downloads, Java runtimes, Fabric, Modrinth, the signed channel, auth, instances, launching. No UI code, no printing.
- `crates/esteban-cli`: a thin console front end. Anything the GUI can do, the CLI can do. It is how launching gets tested without a window.
- `app/src-tauri`: a thin Tauri shell exposing typed commands over `esteban-core`. Capabilities are least-privilege, CSP is strict, nothing is loaded remotely.
- `app/src`: React + TypeScript. Plain CSS with tokens in `styles/tokens.css`, CSS modules per component, fonts bundled locally.

Instances:

- One per game version, loader (Vanilla, Fabric, Forge) and Hacked flag, each in its own folder: `fabric-1.21.4`, `fabric-1.21.4-hacked`, `vanilla-1.8.9`. Folders from before loaders existed (`clean-<v>`, `hacks-<v>`) are renamed once on start, unless the new name is already taken.
- Vanilla gets no mods. Fabric gets the performance mods, plus the HUD mod on the Esteban versions.
- Hacked is opt-in, only exists for the Esteban versions on Fabric, and adds the hacks mod. Every other instance is guarded: the hacks artifact cannot be added, and a pre-launch check refuses to start if one is found in its `mods/`.
- `instance.json` (schema 2) holds the version, loader, loader version, Hacked flag, mod toggles and every managed jar with its hash. Older `instance.json` plus `mods.lock.json` pairs are read and converted, and the old lock goes on the next write.

## Install and launch

`esteban_core::install::install` does everything a launch needs, in order:

1. Version manifest, then the version JSON, checked against the sha1 the manifest lists.
2. Client jar, libraries (the loader's win when both list the same artifact), assets, and the Java runtime named by `javaVersion.component`. Versions before 1.19 also get their native libraries unpacked into the instance's `natives/` (honoring `extract.exclude`, refusing any path that would leave the folder). Versions before 1.7.3 get their assets copied into the old layouts they read (`assets/virtual/legacy/`, or the instance's `resources/` before 1.6).
3. A brand-new instance (no `options.txt` yet) gets `guiScale:3`, written after a `version:` line with the data version read from the client jar's `version.json`. Without that line the game treats the file as very old and runs every options upgrade on it, and one of those turns off Mojang's accessibility screen for new players. An existing `options.txt` is never touched.
4. Fabric only: mods from Modrinth (releases only, sha512 checked), plus the hacks jar for Hacked only. Picks are written to `instance.json` so a re-run is reproducible. `--update` or a changed mod toggle resolves again. See [Mods](#mods).
5. The guard for every instance that isn't Hacked. It opens every jar in `mods/`, including jars nested inside jars, and refuses if the hacks mod is anywhere.

Every download goes to a `.part` file, is checked against its hash and size, and is only then renamed into place. Existing files are re-hashed on each run, so a corrupted file is caught and fetched again.

Launching builds the argument list from the version JSON's rules, adds the loader's arguments, and starts Java from the bundled runtime. Versions before 1.13 use the old one-line `minecraftArguments` with Mojang's classic JVM flags.

- **Memory:** heap size comes from system RAM.
- **GC:** on 26.x the GC flags come from Mojang's own `default-user-jvm` block. On everything older they are G1 with Mojang's tuning.
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

Every skipped mod gets one plain sentence, printed during install and stored in `instance.json`, so it shows up again in `mods list`. If Modrinth can't be reached, the last lock for the same game version and toggles is kept, with a notice. With no lock to fall back on, install stops with a plain message.

Jars the launcher did not put in `mods/` are left alone and listed as "added by you, not checked".

## Forge

Forge is offered from 1.6.1 to 1.20.1. A Forge instance gets Forge and nothing else from the launcher, plus whatever jars you put in `mods/`.

- The installer jar comes from Forge's maven and is checked against the `.sha512` published next to it. The launcher reads it, it never runs Forge's installer UI.
- **New format** (1.12.2, and 1.13 up): every library has a sha1 in the installer. Forge's setup tools (mappings, splitting and patching the game jar) run with the instance's own Java, and every file they produce that the installer lists a hash for (`<KEY>_SHA`) is checked afterwards. On a reinstall where all of those still match, the tools don't run again. The mappings step downloads Mojang's mappings itself, from Mojang.
- **Old format** (1.6.1 to 1.12.1): the Forge jar is taken out of the checked installer. Each library is checked against the sha1s Forge lists for it, or Mojang's hash when the vanilla version lists the same file, or the `.sha1` published next to the file. A library with none of those is refused.
- 1.5.2 Forge is left out: it only starts after the signature files are stripped out of Mojang's game jar, and the launcher doesn't modify Mojang's files.
- The game jar is copied to `versions/<forge id>/<forge id>.jar`, the name Forge's own launch arguments expect.

## Skins

A skin library lives in the settings folder (`skins/<id>.png` plus `skins.json`), so it stays put when the game folder moves. It works offline and needs no account.

- Import checks the file is a real PNG, 64x64 (or the old 64x32), under 1 MB. Classic or slim arms are guessed from the arm pixels and can be changed.
- Each instance picks one skin, or "account skin" for none.
- At launch, Fabric and Forge instances get the pick through [CustomSkinLoader](https://modrinth.com/mod/customskinloader), downloaded from Modrinth like every other mod. The launcher writes its source list to only two entries, your local file first and then Mojang, so none of its other skin sites are ever asked. The skin is copied to `CustomSkinLoader/LocalSkin/skins/<account name>.png`. Only you see it; other players see your Mojang skin.
- Vanilla has no mod to show a local skin, and versions without a CustomSkinLoader build (1.7.10 and older, 26.3 for now) say so instead.
- Uploading a skin to your Mojang account comes with Microsoft sign-in (M2).

## Pack folders

`esteban-cli packs link --from <game folder>` points an instance's `shaderpacks/`, `resourcepacks/` and `screenshots/` at the same folders in another game folder, for example `~/.minecraft` from Lunar or the vanilla launcher. Nothing is copied. It is a symlink on Linux and a directory junction on Windows, so it needs no admin rights.

- `mods/` is never linked, so another launcher's mods can never reach a guarded instance.
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
| Forge installer | the `.sha512` (or `.sha256`, `.sha1`) next to it on Forge's maven |
| Forge libraries and setup outputs | sha1 from the installer. Old installers: Forge's listed sha1s, Mojang's hash, or the `.sha1` next to the file (see [Forge](#forge)) |
| Modrinth mods | sha512 from the Modrinth version |
| Esteban jars (HUD, hacks) | sha256 from `versions.json`: the signed list from GitHub when it passes its checks, otherwise the one compiled in (see [Update channel](update-channel.md)) |
| `versions.json`, `latest.json` | minisign signature against the public keys compiled in from `config/keys.json` |

Not hash-checked, because no source publishes a hash for them: Mojang's version manifest and runtime index, Fabric Meta answers, Modrinth API answers, Forge's version list and recommended builds. They are fetched over HTTPS from the [allowed hosts](network.md) only. The Fabric `.sha256` files come from the same host as the jar, so they catch corruption but not a compromised Maven.

Files already on disk are re-hashed on every install and launch, and a mismatch is fetched again.

## Testing a launch

Launching needs a signed-in Microsoft account (M2). There is no offline mode and no way to build one in.

`--smoke-test` waits until the title screen has loaded, keeps the game up for a few seconds, then stops it and exits 0. It fails if the game crashes or takes too long. For a brand-new instance it also writes `onboardAccessibility:false` into the first `options.txt`, so the title screen is reached. Normal launches never touch that setting.
