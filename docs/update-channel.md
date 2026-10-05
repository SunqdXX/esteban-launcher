# Update channel

## Esteban jars

The launcher fetches `versions.json` and `versions.json.minisig` from GitHub Releases.

- The whole file is signed with minisign (Ed25519). Public keys are compiled into the launcher as a list, so a key can be rotated. The private key is kept offline by the owner and is never in this repo or in CI.
- `sequence` only goes up. The launcher stores the highest value it has accepted and refuses anything lower (rollback).
- `expires` bounds how long a signed file stays valid. An expired file blocks new downloads but does not stop launching jars that were already verified (freeze).
- Each artifact entry has `url`, `sha256` and `size`. After download, the jar must match both or it is deleted.
- `kind` is `clean` or `hacks`. A `hacks` artifact is only ever placed in a hacks profile.

Any failure is a hard stop with a readable message. There is no "continue anyway".

## Launcher updates

Handled by the Tauri updater with its own signing key, separate from the jar key. The updater public key is pinned in the app.

## Threat model

- **Compromised GitHub account or release:** an attacker can replace files but cannot sign them. Signature check fails, nothing is installed.
- **Old but validly signed file replayed:** blocked by `sequence`.
- **Channel frozen to hide a fix:** limited by `expires`.
- **Network attacker:** HTTPS plus signature plus hash.
- **Stolen signing key:** the real risk. Keep it offline, use 2FA everywhere, and rotate via the pinned key list if it ever leaks.
