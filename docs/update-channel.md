# Update channel

## Esteban jars

The launcher fetches `versions.json` and `versions.json.minisig` from GitHub Releases.

- The whole file is signed with minisign (Ed25519). Public keys are compiled into the launcher as a list, so a key can be rotated. The private key is kept offline by the owner and is never in this repo or in CI.
- `sequence` only goes up. The launcher stores the highest value it has accepted and refuses anything lower (rollback).
- `expires` bounds how long a signed file stays valid. An expired file blocks new downloads but does not stop launching jars that were already verified (freeze).
- `builds` has one entry per game version, loader and Hacked flag (`mc`, `loader.kind`, `hacked`), plus the loader range the jars need (`loader.min`) and the loader the tests ran on (`loader.tested`).
- Each build lists its `jars`: `id`, `version`, `file`, `url`, `sha256` and `size`. After download, the jar must match both or it is deleted.
- The hacks jar (`id: "esteban"`) may only appear in a build with `hacked: true`, and only Fabric builds may list jars at all. A file that breaks either rule is refused as a whole, before anything is downloaded.
- `retiredHacksSha256` keeps hashes of older hacks jars, so the guard still recognizes them in a non-Hacked instance.

Until M5 ships the fetch and signature check, a copy of this file (`config/versions.json`, schema 2) is compiled into the launcher and goes through the same checks.

Any failure is a hard stop with a readable message. There is no "continue anyway".

## Launcher updates

Handled by the Tauri updater with its own signing key, separate from the jar key. The updater public key is pinned in the app.

## Threat model

- **Compromised GitHub account or release:** an attacker can replace files but cannot sign them. Signature check fails, nothing is installed.
- **Old but validly signed file replayed:** blocked by `sequence`.
- **Channel frozen to hide a fix:** limited by `expires`.
- **Network attacker:** HTTPS plus signature plus hash.
- **Stolen signing key:** the real risk. Keep it offline, use 2FA everywhere, and rotate via the pinned key list if it ever leaks.
