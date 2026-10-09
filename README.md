# esteban launcher

free minecraft java launcher for windows and linux. still in development, theres nothing to download yet

it installs the game, java, fabric and a set of performance mods straight from their official sources, checks every file against its published hash, then launches. two profiles:

- **Esteban** (default): fabric api, sodium, lithium, ferritecore, immediatelyfast and iris. zero cheat code in it, and the launcher refuses to start it if the hacks mod ever ends up in its mods folder
- **Esteban + Hacks** (opt-in): the same plus the [esteban mod](https://github.com/SunqdXX/esteban). u confirm a one time warning first. most servers ban it, so where u use it is on u

## login

microsoft accounts only, the official way. no cracked or offline accounts

sign in isnt built yet tho, it waits on mojang approving the app

## what works right now

- the command line launcher (`esteban-cli`): install any release as vanilla or fabric, print the exact launch command, turn mods on or off per instance, and use the shader packs, resource packs and screenshots from another launcher (lunar, vanilla) by linking or copying them in
- launching needs the microsoft login above, so for now its only tested locally
- 1.21.4 installs and starts. 26.x installs too, full testing for it comes later
- mods come from modrinth at install time, nothing gets bundled. if a mod has no build for ur version or wouldnt start next to the others, its skipped and u get told why
- the app window has every screen except Servers: Play (pick a version and Normal or Hacked, hit Play and it installs and checks everything with a live progress bar), Profiles, Mods (turn mods on or off), Settings (memory, java, extra jvm flags, where games live, packs from another launcher) and About
- both modes get the esteban hud from the [esteban v1.4.0 release](https://github.com/SunqdXX/esteban/releases/tag/v1.4.0), Hacked also gets the hacks mod. both are hash checked
- Play stops after the install for now and tells u why: starting the game needs the microsoft sign in
- tested on linux. windows builds in ci but hasnt been run in game yet

## what it doesnt do

- no ads, no telemetry, no accounts of our own. it only talks to the hosts listed in [docs/network.md](docs/network.md)
- non-commercial, nothing is for sale

## planned

- signed updates: the update list gets signed and every jar is checked against it before anything installs
- voluntary crypto donations, never needed for anything

## build

needs rust (stable) and node 22. on linux also the webkitgtk 4.1 and libsoup 3 dev packages

```
cd app
npm ci
npm run tauri dev
```

## command line

```
cargo run -p esteban-cli -- install --version 1.21.4
cargo run -p esteban-cli -- install --version 1.8.9 --loader vanilla
cargo run -p esteban-cli -- plan --version 1.21.4 --hacked
cargo run -p esteban-cli -- mods list --version 1.21.4
cargo run -p esteban-cli -- mods disable iris --version 1.21.4
cargo run -p esteban-cli -- packs link --version 1.21.4 --from ~/.minecraft
cargo run -p esteban-cli -- path --version 1.21.4
```

## license

[GPL-3.0](LICENSE). the logo (`app/src/assets/logo/` and the app icons) and the screenshots in `app/src/assets/backgrounds/` are not covered by the GPL, see the `NOTICE` there. fonts are OFL, see [docs/third-party.md](docs/third-party.md)

Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.
