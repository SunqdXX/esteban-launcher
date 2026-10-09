# Releasing the launcher

Only the owner runs these steps. CI never sees a private key: it builds unsigned installers into a draft release, and the signing happens on the owner's machine.

Build the signing tool once (and again after pulling changes to it):

```
cargo build --release -p esteban-sign
```

## Steps

1. Raise the version in all three places, to the same `x.y.z`: `Cargo.toml` (`[workspace.package] version`), `app/package.json` and `app/src-tauri/tauri.conf.json`. Commit, push, wait for CI to pass.
2. Tag it and push the tag:

   ```
   git tag v0.2.0
   git push origin v0.2.0
   ```

   The release workflow checks the tag matches the version, builds the AppImage and deb on Linux and the setup.exe and msi on Windows, and opens a draft release with them and `SHA256SUMS.txt`. Drafts are only visible to the owner.
3. Download the draft and check it arrived intact:

   ```
   gh release download v0.2.0 -R SunqdXX/esteban-launcher -D ~/esteban-release/launcher-0.2.0
   cd ~/esteban-release/launcher-0.2.0 && sha256sum -c SHA256SUMS.txt
   ```

4. Sign. It asks for the updater key password once:

   ```
   ~/WORK/EstebanLauncherMc/target/release/esteban-sign release --key ~/esteban-keys/updater.key --version 0.2.0 --notes "What changed, one line" ~/esteban-release/launcher-0.2.0/*.AppImage ~/esteban-release/launcher-0.2.0/*.deb ~/esteban-release/launcher-0.2.0/*-setup.exe ~/esteban-release/launcher-0.2.0/*.msi
   ```

   This writes a `.sig` next to each installer, plus `latest.json` and `latest.json.minisig`, and checks every signature before it finishes.
5. Upload them and publish:

   ```
   cd ~/esteban-release/launcher-0.2.0
   gh release upload v0.2.0 -R SunqdXX/esteban-launcher *.sig latest.json latest.json.minisig
   gh release edit v0.2.0 -R SunqdXX/esteban-launcher --draft=false
   ```

6. Check it from an older launcher: `esteban-cli update` should say the new version is out, signature checked, and About shows a Download and install button.

## What the launcher checks before installing

1. `latest.json` must carry a minisign signature from the updater key built into the launcher (`config/keys.json`).
2. The updater (`tauri-plugin-updater`) reads the same file. The version, download address and signature it picks must equal an entry of the signed copy, or nothing is installed.
3. The downloaded installer must be signed by the updater key pinned in `tauri.conf.json` (the same key, a test keeps them equal), and its signed version must equal the announced one. Older versions are never installed.

An AppImage replaces itself and asks for a restart. A deb install asks for the system password. On Windows the installer runs and the launcher restarts. A copy that wasn't installed from one of these packages says it can't update itself and links the release page.

## Esteban version list

See [update-channel.md](update-channel.md).
