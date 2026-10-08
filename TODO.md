# TODO

## Placeholders

- [ ] Play, Profiles, Mods, Servers, Settings and About are empty screens that say "Not built yet". They are built in M4, after the Play screen mockup is approved.
- [ ] The disclaimer lives in the sidebar footer. The About screen copy comes in M4.
- [ ] The hacks jar comes from the pinned v1.3.0 release (`esteban.rs`, hash checked). It is replaced by the signed `versions.json` channel in M5.
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
- [ ] `config/donate.json` with public BTC, XMR and USDC addresses (M4).
- [ ] Signing keys for `versions.json` and the updater, kept offline (M5, M7).

## Later

- [ ] Code-signing certificate for Windows installers (M7).
