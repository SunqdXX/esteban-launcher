use std::collections::HashMap;

use crate::mojang::args;
use crate::mojang::rules::Context;
use crate::mojang::version::VersionJson;

const MIB: u64 = 1024 * 1024;

pub const G1_FLAGS: &[&str] = &[
    "-XX:+UseG1GC",
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:G1NewSizePercent=20",
    "-XX:G1ReservePercent=20",
    "-XX:MaxGCPauseMillis=50",
    "-XX:G1HeapRegionSize=32M",
];

pub fn heap_mb(total_memory_bytes: u64) -> (u64, u64) {
    let total = total_memory_bytes / MIB;
    let tier = match total {
        t if t < 6 * 1024 => 2048,
        t if t < 12 * 1024 => 3072,
        t if t < 24 * 1024 => 4096,
        _ => 6144,
    };
    let max = tier.min(total / 2).max(1024);
    (max.min(2048), max)
}

pub fn default_flags(version: &VersionJson, ctx: &Context<'_>, max_heap_mb: u64) -> Vec<String> {
    let min_heap_mb = max_heap_mb.min(2048);
    let mut flags = vec![format!("-Xms{min_heap_mb}M"), format!("-Xmx{max_heap_mb}M")];
    let mojang_defaults = version
        .arguments
        .as_ref()
        .map(|a| a.default_user_jvm.as_slice())
        .unwrap_or_default();
    if mojang_defaults.is_empty() {
        flags.extend(G1_FLAGS.iter().map(|f| (*f).to_string()));
    } else {
        flags.extend(
            args::expand(mojang_defaults, ctx, &HashMap::new())
                .into_iter()
                .filter(|f| !f.starts_with("-Xms") && !f.starts_with("-Xmx")),
        );
    }
    flags
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::system::{Arch, OsName, Platform};

    const GIB: u64 = 1024 * MIB;

    #[test]
    fn heap_tiers_never_take_more_than_half_of_ram() {
        assert_eq!(heap_mb(4 * GIB), (2048, 2048));
        assert_eq!(heap_mb(3 * GIB), (1536, 1536));
        assert_eq!(heap_mb(8 * GIB), (2048, 3072));
        assert_eq!(heap_mb(16 * GIB), (2048, 4096));
        assert_eq!(heap_mb(64 * GIB), (2048, 6144));
    }

    fn version(arguments: &str) -> VersionJson {
        serde_json::from_str(&format!(
            r#"{{"id":"x","type":"release","mainClass":"m","arguments":{arguments},"assetIndex":{{"id":"1","sha1":"a","size":1,"totalSize":1,"url":"https://piston-meta.mojang.com/a"}},"assets":"1","downloads":{{"client":{{"sha1":"a","size":1,"url":"https://piston-data.mojang.com/c"}}}},"javaVersion":{{"component":"java-runtime-epsilon","majorVersion":25}},"libraries":[]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn uses_mojangs_defaults_when_present_but_our_heap() {
        let v = version(
            r#"{"game":[],"jvm":[],"default-user-jvm":[{"value":["-Xms2G","-Xmx4G","-XX:+UseCompactObjectHeaders"]},{"rules":[{"action":"allow","os":{"name":"linux"}}],"value":["-XX:+UseZGC"]}]}"#,
        );
        let linux = Platform {
            os: OsName::Linux,
            arch: Arch::X86_64,
            os_version: None,
        };
        let features = BTreeSet::new();
        let flags = default_flags(
            &v,
            &Context {
                platform: &linux,
                features: &features,
            },
            4096,
        );
        assert_eq!(
            flags,
            vec![
                "-Xms2048M",
                "-Xmx4096M",
                "-XX:+UseCompactObjectHeaders",
                "-XX:+UseZGC"
            ]
        );
    }

    #[test]
    fn falls_back_to_g1_without_mojang_defaults() {
        let v = version(r#"{"game":[],"jvm":[]}"#);
        let linux = Platform {
            os: OsName::Linux,
            arch: Arch::X86_64,
            os_version: None,
        };
        let features = BTreeSet::new();
        let flags = default_flags(
            &v,
            &Context {
                platform: &linux,
                features: &features,
            },
            3072,
        );
        assert_eq!(&flags[..3], &["-Xms2048M", "-Xmx3072M", "-XX:+UseG1GC"]);
    }
}
