# esteban launcher

free minecraft java launcher for windows and linux. still in development, theres nothing to download yet

it installs any minecraft release (1.0 up to 26.3) straight from mojang, java, and a loader, checks every file against its published hash, then launches. every version + loader gets its own folder:

- **vanilla**: just the game, any release
- **fabric** (1.14 and up): fabric api, sodium, lithium, ferritecore, immediatelyfast and iris, plus the esteban hud on the 5 esteban versions (26.3, 26.2, 26.1.2, 26.1, 1.21.4). zero cheat code in it, and the launcher refuses to start it if the hacks mod ever ends up in its mods folder
- **forge** (1.6.1 to 1.20.1): forge and nothing else, drop ur own mods in
- **hacked** (opt-in, only the 5 esteban versions on fabric): the same as fabric plus the [esteban mod](https://github.com/SunqdXX/esteban). u confirm a one time warning first. most servers ban it, so where u use it is on u

skins: import pngs, keep as many as u want and pick one per instance. fabric and forge show it through [customskinloader](https://modrinth.com/mod/customskinloader), only u see it, no sign in needed

## login

microsoft accounts only, the official way. no cracked or offline accounts

sign in isnt built yet tho, it waits on mojang approving the app

## what works right now

- the command line launcher (`esteban-cli`): install any release as vanilla, fabric or forge, pick a loader build, manage skins, print the exact launch command, turn mods on or off per instance, and use the shader packs, resource packs and screenshots from another launcher (lunar, vanilla) by linking or copying them in
- launching needs the microsoft login above, so for now its only tested locally
- tested starting up: vanilla 1.5.2, 1.6.4, 1.8.9, 1.16.5, fabric 1.21.4, forge 1.6.1, 1.6.4, 1.7.10, 1.8.9, 1.12.2 and 1.20.1
- mods come from modrinth at install time, nothing gets bundled. if a mod has no build for ur version or wouldnt start next to the others, its skipped and u get told why
- the app window has every screen except Servers: Play (pick any release, the loader, Normal or Hacked where it exists, hit Play and it installs and checks everything with a live progress bar), Profiles (every instance with its loader, build and skin), Mods (turn mods on or off, pick a loader build), Skins, Settings (memory, java, extra jvm flags, where games live, packs from another launcher) and About
- fabric on the esteban versions gets the esteban hud from the [esteban v1.4.0 release](https://github.com/SunqdXX/esteban/releases/tag/v1.4.0), Hacked also gets the hacks mod. both are hash checked
- Play stops after the install for now and tells u why: starting the game needs the microsoft sign in
- tested on linux. windows builds in ci but hasnt been run in game yet

## what it doesnt do

- no ads, no telemetry, no accounts of our own. it only talks to the hosts listed in [docs/network.md](docs/network.md)
- non-commercial, nothing is for sale

## signed releases

the list of esteban jars (`versions.json`) and the launcher update info (`latest.json`) are signed. the launcher only trusts them if the signature matches a public key built into it, the list isnt older than one it already saw, and it hasnt expired. how it works and how releases get signed: [docs/update-channel.md](docs/update-channel.md)

the launcher updates itself from signed releases: About shows when a new version is out and installs it after checking the signature (AppImage, deb and the windows installer). how a release gets built and signed: [docs/release.md](docs/release.md)

## planned

- voluntary crypto donations, never needed for anything

## build

needs rust (stable) and node 22. on linux also the webkitgtk 4.1 and libsoup 3 dev packages

```
cd app
npm ci
npm run tauri dev
```

a release binary without installers lands in `target/release/esteban-launcher`:

```
cd app
npm ci
npm run tauri build -- --no-bundle
```

plain `cargo build --release` works too, as long as `npm run build` ran in `app/` after the last frontend change (the build stops and says so otherwise)

## command line

```
cargo run -p esteban-cli -- install --version 1.21.4
cargo run -p esteban-cli -- install --version 1.8.9 --loader vanilla
cargo run -p esteban-cli -- install --version 1.20.1 --loader forge
cargo run -p esteban-cli -- versions
cargo run -p esteban-cli -- loaders --version 1.20.1 --loader forge
cargo run -p esteban-cli -- skin add ~/skins/mine.png
cargo run -p esteban-cli -- channel
cargo run -p esteban-cli -- update
cargo run -p esteban-cli -- plan --version 1.21.4 --hacked
cargo run -p esteban-cli -- mods list --version 1.21.4
cargo run -p esteban-cli -- mods disable iris --version 1.21.4
cargo run -p esteban-cli -- packs link --version 1.21.4 --from ~/.minecraft
cargo run -p esteban-cli -- path --version 1.21.4
```

## license

[GPL-3.0](LICENSE). the logo (`app/src/assets/logo/` and the app icons) and the screenshots in `app/src/assets/backgrounds/` are not covered by the GPL, see the `NOTICE` there. fonts are OFL, see [docs/third-party.md](docs/third-party.md)

Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.
