use std::collections::HashMap;

use super::rules::{self, Context};
use super::version::Argument;

pub fn expand(args: &[Argument], ctx: &Context<'_>, vars: &HashMap<&str, String>) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            Argument::Plain(value) => out.push(substitute(value, vars)),
            Argument::Ruled { rules, value } => {
                if rules::allowed(rules, ctx) {
                    out.extend(value.values().iter().map(|v| substitute(v, vars)));
                }
            }
        }
    }
    out
}

const LEGACY_JVM: &str = r#"[
    {"rules":[{"action":"allow","os":{"name":"osx"}}],"value":["-XstartOnFirstThread"]},
    {"rules":[{"action":"allow","os":{"name":"windows"}}],"value":"-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump"},
    {"rules":[{"action":"allow","os":{"arch":"x86"}}],"value":"-Xss1M"},
    "-Djava.library.path=${natives_directory}",
    "-Dminecraft.launcher.brand=${launcher_name}",
    "-Dminecraft.launcher.version=${launcher_version}",
    "-cp",
    "${classpath}"
]"#;

pub fn legacy_jvm() -> Vec<Argument> {
    serde_json::from_str(LEGACY_JVM).unwrap_or_default()
}

pub fn legacy_game(line: &str, vars: &HashMap<&str, String>) -> Vec<String> {
    line.split_whitespace()
        .map(|part| substitute(part, vars))
        .collect()
}

pub fn substitute(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(value) => out.push_str(value),
                    None => {
                        tracing::warn!(placeholder = key, "unknown launch placeholder");
                        out.push_str(&rest[start..start + 2 + end + 1]);
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::system::{Arch, OsName, Platform};

    #[test]
    fn substitutes_known_and_keeps_unknown_placeholders() {
        let vars = HashMap::from([
            ("a", "1".to_string()),
            ("natives_directory", "/n".to_string()),
        ]);
        assert_eq!(substitute("x${a}y", &vars), "x1y");
        assert_eq!(substitute("${natives_directory}/lwjgl", &vars), "/n/lwjgl");
        assert_eq!(substitute("${nope}", &vars), "${nope}");
        assert_eq!(substitute("plain", &vars), "plain");
    }

    #[test]
    fn old_versions_get_mojangs_classic_jvm_flags_and_split_game_args() {
        let platform = Platform {
            os: OsName::Linux,
            arch: Arch::X86_64,
            os_version: None,
        };
        let features = BTreeSet::new();
        let ctx = Context {
            platform: &platform,
            features: &features,
        };
        let vars = HashMap::from([
            ("natives_directory", "/i/natives".to_string()),
            ("launcher_name", "esteban-launcher".to_string()),
            ("launcher_version", "0.1.0".to_string()),
            ("classpath", "/a.jar:/b.jar".to_string()),
            ("auth_player_name", "Steve".to_string()),
            ("auth_session", "token:t:u".to_string()),
            ("game_directory", "/i".to_string()),
            ("game_assets", "/i/resources".to_string()),
        ]);
        assert_eq!(legacy_jvm().len(), 8);
        assert_eq!(
            expand(&legacy_jvm(), &ctx, &vars),
            vec![
                "-Djava.library.path=/i/natives",
                "-Dminecraft.launcher.brand=esteban-launcher",
                "-Dminecraft.launcher.version=0.1.0",
                "-cp",
                "/a.jar:/b.jar"
            ]
        );
        assert_eq!(
            legacy_game(
                "${auth_player_name} ${auth_session}  --gameDir ${game_directory} --assetsDir ${game_assets}",
                &vars
            ),
            vec![
                "Steve",
                "token:t:u",
                "--gameDir",
                "/i",
                "--assetsDir",
                "/i/resources"
            ]
        );
    }

    #[test]
    fn values_without_rules_always_apply() {
        let args: Vec<Argument> =
            serde_json::from_str(r#"[{"value":["-Xms2G","-Xmx4G"]}]"#).unwrap();
        let platform = Platform {
            os: OsName::Windows,
            arch: Arch::X86_64,
            os_version: None,
        };
        let features = BTreeSet::new();
        let ctx = Context {
            platform: &platform,
            features: &features,
        };
        assert_eq!(
            expand(&args, &ctx, &HashMap::new()),
            vec!["-Xms2G", "-Xmx4G"]
        );
    }

    #[test]
    fn expands_rules_features_and_values() {
        let json = r#"[
            "--username", "${auth_player_name}",
            {"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"},
            {"rules":[{"action":"allow","features":{"is_quick_play_multiplayer":true}}],"value":["--quickPlayMultiplayer","${quickPlayMultiplayer}"]},
            {"rules":[{"action":"allow","os":{"name":"osx"}}],"value":["-XstartOnFirstThread"]}
        ]"#;
        let args: Vec<Argument> = serde_json::from_str(json).unwrap();
        let platform = Platform {
            os: OsName::Linux,
            arch: Arch::X86_64,
            os_version: None,
        };
        let features = BTreeSet::from(["is_quick_play_multiplayer".to_string()]);
        let vars = HashMap::from([
            ("auth_player_name", "Steve".to_string()),
            ("quickPlayMultiplayer", "play.example:25565".to_string()),
        ]);
        let ctx = Context {
            platform: &platform,
            features: &features,
        };
        assert_eq!(
            expand(&args, &ctx, &vars),
            vec![
                "--username",
                "Steve",
                "--quickPlayMultiplayer",
                "play.example:25565"
            ]
        );
    }
}
