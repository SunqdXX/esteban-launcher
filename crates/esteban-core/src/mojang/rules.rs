use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::system::{Arch, OsName, Platform, parse_version};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: Action,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<BTreeMap<String, bool>>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(rename = "versionRange", default)]
    pub version_range: Option<VersionRange>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionRange {
    #[serde(default)]
    pub min: Option<String>,
    #[serde(default)]
    pub max: Option<String>,
}

pub struct Context<'a> {
    pub platform: &'a Platform,
    pub features: &'a BTreeSet<String>,
}

pub fn allowed(rules: &[Rule], ctx: &Context<'_>) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allow = false;
    for rule in rules {
        if rule.matches(ctx) {
            allow = rule.action == Action::Allow;
        }
    }
    allow
}

impl Rule {
    fn matches(&self, ctx: &Context<'_>) -> bool {
        if !self.unknown.is_empty() {
            return false;
        }
        let os_ok = self.os.as_ref().is_none_or(|os| os.matches(ctx.platform));
        let features_ok = self.features.as_ref().is_none_or(|features| {
            features
                .iter()
                .all(|(name, wanted)| ctx.features.contains(name) == *wanted)
        });
        os_ok && features_ok
    }
}

impl OsRule {
    fn matches(&self, platform: &Platform) -> bool {
        if !self.unknown.is_empty() || self.version.is_some() {
            return false;
        }
        let name_ok = self.name.as_deref().is_none_or(|name| {
            matches!(
                (name, platform.os),
                ("windows", OsName::Windows) | ("linux", OsName::Linux) | ("osx", OsName::Osx)
            )
        });
        let arch_ok = self.arch.as_deref().is_none_or(|arch| {
            matches!(
                (arch, platform.arch),
                ("x86", Arch::X86)
                    | ("x86_64", Arch::X86_64)
                    | ("arm64" | "aarch64", Arch::Aarch64)
            )
        });
        let range_ok = self
            .version_range
            .as_ref()
            .is_none_or(|range| range.contains(platform.os_version.as_deref()));
        name_ok && arch_ok && range_ok
    }
}

impl VersionRange {
    fn contains(&self, version: Option<&[u64]>) -> bool {
        let Some(version) = version else {
            return false;
        };
        let at_least_min = self
            .min
            .as_deref()
            .and_then(parse_version)
            .is_none_or(|min| version >= min.as_slice());
        let below_max = self
            .max
            .as_deref()
            .and_then(parse_version)
            .is_none_or(|max| version < max.as_slice());
        at_least_min && below_max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    fn platform(os: OsName, arch: Arch, version: Option<Vec<u64>>) -> Platform {
        Platform {
            os,
            arch,
            os_version: version,
        }
    }

    fn check(json: &str, platform: &Platform, features: &[&str]) -> bool {
        let features: BTreeSet<String> = features.iter().map(|f| (*f).to_string()).collect();
        allowed(
            &rules(json),
            &Context {
                platform,
                features: &features,
            },
        )
    }

    const ZGC: &str = r#"[{"action":"allow","os":{"name":"osx"}},{"action":"allow","os":{"name":"linux"}},{"action":"allow","os":{"name":"windows","versionRange":{"min":"10.0.17134"}}}]"#;
    const OLD_WINDOWS_G1: &str =
        r#"[{"action":"allow","os":{"name":"windows","versionRange":{"max":"10.0.17134"}}}]"#;

    #[test]
    fn empty_rules_allow() {
        let linux = platform(OsName::Linux, Arch::X86_64, None);
        assert!(check("[]", &linux, &[]));
    }

    #[test]
    fn os_name_rules() {
        let linux = platform(OsName::Linux, Arch::X86_64, None);
        let windows = platform(OsName::Windows, Arch::X86_64, None);
        let only_windows = r#"[{"action":"allow","os":{"name":"windows"}}]"#;
        assert!(!check(only_windows, &linux, &[]));
        assert!(check(only_windows, &windows, &[]));
    }

    #[test]
    fn x86_means_a_32_bit_runtime() {
        let rule = r#"[{"action":"allow","os":{"arch":"x86"}}]"#;
        assert!(!check(
            rule,
            &platform(OsName::Linux, Arch::X86_64, None),
            &[]
        ));
        assert!(check(
            rule,
            &platform(OsName::Windows, Arch::X86, None),
            &[]
        ));
    }

    #[test]
    fn later_rules_win() {
        let rule = r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#;
        assert!(check(
            rule,
            &platform(OsName::Linux, Arch::X86_64, None),
            &[]
        ));
        assert!(!check(
            rule,
            &platform(OsName::Osx, Arch::Aarch64, None),
            &[]
        ));
    }

    #[test]
    fn version_range_min_is_inclusive_and_max_exclusive() {
        let new = platform(OsName::Windows, Arch::X86_64, Some(vec![10, 0, 19045]));
        let edge = platform(OsName::Windows, Arch::X86_64, Some(vec![10, 0, 17134]));
        let old = platform(OsName::Windows, Arch::X86_64, Some(vec![10, 0, 16299]));
        assert!(check(ZGC, &new, &[]) && !check(OLD_WINDOWS_G1, &new, &[]));
        assert!(check(ZGC, &edge, &[]) && !check(OLD_WINDOWS_G1, &edge, &[]));
        assert!(!check(ZGC, &old, &[]) && check(OLD_WINDOWS_G1, &old, &[]));
    }

    #[test]
    fn unknown_os_version_never_matches_a_range() {
        let windows = platform(OsName::Windows, Arch::X86_64, None);
        assert!(!check(ZGC, &windows, &[]));
        assert!(!check(OLD_WINDOWS_G1, &windows, &[]));
        assert!(check(
            ZGC,
            &platform(OsName::Linux, Arch::X86_64, None),
            &[]
        ));
    }

    #[test]
    fn feature_rules() {
        let linux = platform(OsName::Linux, Arch::X86_64, None);
        let rule = r#"[{"action":"allow","features":{"is_quick_play_multiplayer":true}}]"#;
        assert!(!check(rule, &linux, &[]));
        assert!(check(rule, &linux, &["is_quick_play_multiplayer"]));
        let demo = r#"[{"action":"allow","features":{"is_demo_user":true}}]"#;
        assert!(!check(demo, &linux, &["has_custom_resolution"]));
    }

    #[test]
    fn unknown_conditions_do_not_match() {
        let linux = platform(OsName::Linux, Arch::X86_64, None);
        assert!(!check(
            r#"[{"action":"allow","os":{"name":"linux","flavor":"x"}}]"#,
            &linux,
            &[]
        ));
        assert!(!check(
            r#"[{"action":"allow","os":{"name":"linux","version":"^5"}}]"#,
            &linux,
            &[]
        ));
        assert!(!check(r#"[{"action":"allow","moon":"full"}]"#, &linux, &[]));
    }
}
