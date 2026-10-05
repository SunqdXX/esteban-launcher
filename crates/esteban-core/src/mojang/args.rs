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
