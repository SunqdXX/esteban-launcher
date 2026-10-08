use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use crate::account::Session;
use crate::install::Installed;
use crate::mojang::args;
use crate::mojang::rules::Context;
use crate::system::Platform;
use crate::{LAUNCHER_BRAND, Result, VERSION};

#[derive(Clone, Debug)]
pub enum QuickPlay {
    Singleplayer(String),
    Multiplayer(String),
}

#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    pub quick_play: Option<QuickPlay>,
    pub max_heap_mb: Option<u64>,
    pub extra_jvm_args: Vec<String>,
    pub java: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct LaunchPlan {
    pub java: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    secrets: Vec<String>,
}

impl LaunchPlan {
    pub fn command_line(&self) -> String {
        let mut parts = vec![quote(&self.java.to_string_lossy())];
        let mut hide_next = false;
        for arg in &self.args {
            let secret = hide_next
                || self.secrets.iter().any(|s| {
                    !s.is_empty() && (arg == s || (s.len() >= 16 && arg.contains(s.as_str())))
                });
            hide_next = arg == "--accessToken";
            parts.push(if secret {
                "<redacted>".to_string()
            } else {
                quote(arg)
            });
        }
        parts.join(" ")
    }
}

fn quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_=./:+@,${}<>".contains(c))
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

pub fn build(
    installed: &Installed,
    session: &Session,
    options: &LaunchOptions,
    platform: &Platform,
) -> Result<LaunchPlan> {
    let instance = &installed.instance;
    let mut features = BTreeSet::new();
    let mut vars: HashMap<&str, String> = HashMap::new();
    match &options.quick_play {
        Some(QuickPlay::Singleplayer(world)) => {
            features.insert("has_quick_plays_support".to_string());
            features.insert("is_quick_play_singleplayer".to_string());
            vars.insert("quickPlaySingleplayer", world.clone());
        }
        Some(QuickPlay::Multiplayer(server)) => {
            features.insert("has_quick_plays_support".to_string());
            features.insert("is_quick_play_multiplayer".to_string());
            vars.insert("quickPlayMultiplayer", server.clone());
        }
        None => {}
    }
    vars.insert(
        "quickPlayPath",
        instance
            .dir
            .join("quickPlay")
            .join("log.json")
            .to_string_lossy()
            .into_owned(),
    );

    let classpath: Vec<String> = installed
        .classpath
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let separator = platform.classpath_separator();
    vars.insert("auth_player_name", session.username.clone());
    vars.insert("version_name", installed.fabric.id.clone());
    vars.insert(
        "game_directory",
        instance.dir.to_string_lossy().into_owned(),
    );
    vars.insert(
        "assets_root",
        installed.assets_root.to_string_lossy().into_owned(),
    );
    vars.insert("assets_index_name", installed.version.assets.clone());
    vars.insert("auth_uuid", session.uuid.clone());
    vars.insert(
        "auth_access_token",
        session.access_token.expose().to_string(),
    );
    vars.insert("clientid", session.client_id.clone());
    vars.insert("auth_xuid", session.xuid.clone());
    vars.insert("user_type", session.user_type.clone());
    vars.insert("version_type", installed.version.kind.clone());
    vars.insert(
        "natives_directory",
        instance.natives_dir().to_string_lossy().into_owned(),
    );
    vars.insert("launcher_name", LAUNCHER_BRAND.to_string());
    vars.insert("launcher_version", VERSION.to_string());
    vars.insert("classpath", classpath.join(separator));
    vars.insert("classpath_separator", separator.to_string());
    vars.insert(
        "library_directory",
        installed.libraries_root.to_string_lossy().into_owned(),
    );

    let ctx = Context {
        platform,
        features: &features,
    };
    let arguments = installed.version.arguments.clone().unwrap_or_default();
    let max_heap = options.max_heap_mb.unwrap_or(installed.default_max_heap_mb);

    let mut out = super::jvm::default_flags(&installed.version, &ctx, max_heap);
    out.extend(options.extra_jvm_args.iter().cloned());
    out.extend(args::expand(&arguments.jvm, &ctx, &vars));
    out.extend(
        installed
            .fabric
            .arguments
            .jvm
            .iter()
            .map(|a| args::substitute(a, &vars)),
    );
    out.push(installed.fabric.main_class.clone());
    out.extend(args::expand(&arguments.game, &ctx, &vars));
    out.extend(
        installed
            .fabric
            .arguments
            .game
            .iter()
            .map(|a| args::substitute(a, &vars)),
    );

    Ok(LaunchPlan {
        java: options
            .java
            .clone()
            .unwrap_or_else(|| installed.java.executable.clone()),
        args: out,
        cwd: instance.dir.clone(),
        secrets: vec![session.access_token.expose().to_string()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(args: &[&str], secret: &str) -> LaunchPlan {
        LaunchPlan {
            java: PathBuf::from("/rt/bin/java"),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            cwd: PathBuf::from("/i"),
            secrets: vec![secret.to_string()],
        }
    }

    #[test]
    fn redaction_hides_tokens_but_not_ordinary_args() {
        let short = plan(&["-Xms2048M", "--accessToken", "0", "--uuid", "abc0"], "0");
        assert_eq!(
            short.command_line(),
            "/rt/bin/java -Xms2048M --accessToken <redacted> --uuid abc0"
        );
        let jwt = "eyJhbGciOiJIUzI1NiJ9.payload.signature";
        let long = plan(&["--accessToken", jwt, &format!("-Dx={jwt}")], jwt);
        assert_eq!(
            long.command_line(),
            "/rt/bin/java --accessToken <redacted> <redacted>"
        );
    }

    #[test]
    fn arguments_with_spaces_are_quoted() {
        let p = plan(&["-DFabricMcEmu= net.minecraft.client.main.Main "], "zzzz");
        assert_eq!(
            p.command_line(),
            "/rt/bin/java '-DFabricMcEmu= net.minecraft.client.main.Main '"
        );
    }
}
