# TODO

## Placeholders

- [ ] Servers is still an empty screen. Saved servers and Launch and join come in M6, after sign-in.
- [ ] Play installs and verifies, then stops: launching needs Microsoft sign-in (M2). The account card on Play and the Account section in Settings are placeholders with disabled Sign in buttons.
- [ ] The disclaimer lives in the sidebar footer. The About screen copy comes in M4.
- [ ] No signing key is in `config/keys.json` yet, so the launcher only uses its compiled-in `versions.json` and doesn't check for updates. Both start working once the public keys are added and a signed `versions.json` / `latest.json` is published.
- [ ] "Upload to my Minecraft account" on the Skins screen is disabled until Microsoft sign-in (M2).
- [ ] A Java picked in Settings is checked to be Java, but not yet against the major version each Minecraft version needs. A wrong one fails at launch with Java's own error.
- [ ] Launching needs Microsoft sign-in (M2). Until then only install, plan and mods work.

## Known gaps

- [ ] Local skins need CustomSkinLoader, which has no build for 26.3 or 1.7.10 and older yet. Those say so on the Skins screen.
- [ ] Forge 1.5.2 is left out: it only starts with the signature files stripped out of Mojang's game jar.
- [ ] Not every one of the 103 releases has been launched. Started so far: vanilla 1.5.2, 1.6.4, 1.8.9, 1.16.5, Fabric 1.21.4, Forge 1.6.1, 1.6.4, 1.7.10, 1.8.9, 1.12.2, 1.20.1.
- [ ] Versions 1.12.2 and older use LWJGL 2, which needs XWayland and the `xrandr` command on Linux.

- [ ] Windows build number is not read yet, so Mojang's `versionRange` rules (ZGC on 26.x) never match on Windows and the JVM default GC is used there. Needed for M6.
- [ ] No launch test in CI yet. It needs a signed-in account, so it comes after M2.
- [ ] Every launch needs the network (manifests are fetched fresh). Offline launching is not planned yet.
- [ ] Java runtime files are fetched raw. LZMA would save about 30 MB on a first install.
- [ ] When a mod's jar rules out the newest build of its dependency, the mod is skipped. Picking an older build of the dependency that fits is not tried.
- [ ] A changed mod toggle resolves every mod again, so other mods can move to newer releases at the same time.
- [ ] Pack folder links on Windows are directory junctions. They build in CI but haven't been tried on a real Windows install yet.

## Needs the owner

- [ ] Mojang approval of the registered client ID. Blocks M2.
- [ ] App icon: right now it is the full logo on a square canvas of its own background color. Decide on a square cut of the logo for small icon sizes.
- [x] BTC and XMR addresses and the Discord invite are in `config/donate.json`.
- [ ] USDC address and its network. Until then About leaves the USDC card out; filling `usdc` in `config/donate.json` brings it back.
- [x] Channel and updater keys made by the owner, public keys in `config/keys.json` (channel F06D6DD40133B8FC, updater 1B5726E0D3D4ECBE).
- [ ] Decide where launcher downloads live before the first public release (separate download repo). The release workflow, the updater endpoint and `latest.json` use the launcher repo's releases until then.
- [ ] Windows installers aren't code-signed, so SmartScreen warns on first run. A code-signing certificate fixes that.
