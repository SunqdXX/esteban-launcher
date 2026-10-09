# Update channel

## Esteban jars

The launcher fetches `versions.json` and `versions.json.minisig` from the latest Esteban release on GitHub (`releases/latest/download/`).

- The whole file is signed with minisign (Ed25519). Public keys are compiled into the launcher from `config/keys.json`, as a list so a key can be rotated. The private key is kept offline by the owner and is never in this repo or in CI.
- `sequence` only goes up. The launcher stores the highest value it has accepted (`channel-state.json` in the settings folder) and ignores anything lower (rollback).
- `expires` bounds how long a signed file stays valid. An expired file is ignored (freeze).
- A file that fails any check is never used. The launcher says why and keeps the last good copy it saved, or the list compiled into the binary. Offline, or with nothing published, the same fallback applies. The compiled-in list ships inside the binary and doesn't expire.
- `builds` has one entry per game version, loader and Hacked flag (`mc`, `loader.kind`, `hacked`), plus the loader range the jars need (`loader.min`) and the loader the tests ran on (`loader.tested`).
- Each build lists its `jars`: `id`, `version`, `file`, `url`, `sha256` and `size`. After download, the jar must match both or it is deleted.
- The hacks jar (`id: "esteban"`) may only appear in a build with `hacked: true`, and only Fabric builds may list jars at all. A file that breaks either rule is refused as a whole, before anything is downloaded.
- `retiredHacksSha256` keeps hashes of older hacks jars. The guard counts a hash known to either the compiled-in list or a newer verified one, so a newer list can never shrink it.

Any failure is a hard stop for that file with a readable message. There is no "continue anyway".

## Launcher updates

`latest.json` (the Tauri updater format) and `latest.json.minisig` from the latest launcher release, signed with the updater key, a separate keypair from the channel key. The launcher checks the signature before it reads the version, and About shows the result. Downloading and installing an update comes with the installers (M7), where Tauri's updater checks each installer against the same key.

## Signing (owner only)

`esteban-sign` is a separate tool in this repo. It is never part of the launcher.

```
cargo run --release -p esteban-sign -- keygen --for channel --out <folder>/channel.key
cargo run --release -p esteban-sign -- keygen --for updater --out <folder>/updater.key
```

Each keygen asks for a password (not shown while typing), writes the encrypted secret key with owner-only permissions, writes the public key next to it as `.pub`, and prints only the public key. It refuses to overwrite an existing key. The printed public key goes into `config/keys.json`.

Publishing a new Esteban version list:

1. Edit `config/versions.json`: raise `sequence`, set `issued` to now and `expires` ahead (dates as `YYYY-MM-DDTHH:MM:SSZ`).
2. `cargo run --release -p esteban-sign -- sign --key <folder>/channel.key config/versions.json`. It refuses a file the launcher would refuse, then writes `config/versions.json.minisig`.
3. `cargo run --release -p esteban-sign -- verify config/versions.json` checks it against the keys built into this checkout.
4. Upload `versions.json` and `versions.json.minisig` to the Esteban release on GitHub.

`latest.json` is signed the same way with the updater key.

## Threat model

- **Compromised GitHub account or release:** an attacker can replace files but cannot sign them. Signature check fails, nothing is installed.
- **Old but validly signed file replayed:** blocked by `sequence`.
- **Channel frozen to hide a fix:** limited by `expires`.
- **Network attacker:** HTTPS plus signature plus hash.
- **Stolen signing key:** the real risk. Keep it offline, use 2FA everywhere, and rotate via the pinned key list if it ever leaks.
