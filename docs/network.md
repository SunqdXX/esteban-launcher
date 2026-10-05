# Network

Every outbound host the launcher talks to. Anything not on this list is a bug.

All requests are HTTPS, have timeouts, and send a fixed `User-Agent` that names the launcher and its version.

| Host | Why |
|---|---|
| `piston-meta.mojang.com` | version manifest, version JSONs, asset indexes, Java runtime manifests |
| `piston-data.mojang.com` | client jar, logging config, Java runtime files |
| `libraries.minecraft.net` | game libraries |
| `resources.download.minecraft.net` | asset objects |
| `meta.fabricmc.net` | Fabric loader versions and launch profiles |
| `maven.fabricmc.net` | Fabric loader, intermediary and their libraries |
| `api.modrinth.com` | mod version lookups |
| `cdn.modrinth.com` | mod files |
| `login.microsoftonline.com` | Microsoft sign-in (device code) |
| `user.auth.xboxlive.com` | Xbox Live user token |
| `xsts.auth.xboxlive.com` | XSTS token |
| `api.minecraftservices.com` | game login, ownership check, profile |
| `github.com` | Esteban release channel and launcher updates |
| `objects.githubusercontent.com`, `release-assets.githubusercontent.com` | GitHub release asset downloads (redirect targets of `github.com`) |

No telemetry, no analytics, no other hosts.
