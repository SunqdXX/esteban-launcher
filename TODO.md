# TODO

## Placeholders

- [ ] Servers is still an empty screen. Saved servers and Launch and join come in M6, after sign-in.
- [ ] Play installs and verifies, then stops: launching needs Microsoft sign-in (M2). The account card on Play and the Account section in Settings are placeholders with disabled Sign in buttons.
- [ ] The disclaimer lives in the sidebar footer. The About screen copy comes in M4.
- [ ] The HUD jar (both modes) and the hacks jar (Hacked only) come from the pinned esteban v1.4.0 release (`esteban.rs`, sha256 checked). They are replaced by the signed `versions.json` channel in M5. The v1.3.0 hacks hashes stay on the clean guard's list.
- [ ] A Java picked in Settings is checked to be Java, but not yet against the major version each Minecraft version needs. A wrong one fails at launch with Java's own error.
- [ ] Launching needs Microsoft sign-in (M2). Until then only install, plan and mods work.

## Known gaps

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
- [ ] USDC address and its network. Until then About shows "Address not set yet" for USDC.
- [ ] Signing keys for `versions.json` and the updater, kept offline (M5, M7).

## Later

- [ ] Code-signing certificate for Windows installers (M7).
