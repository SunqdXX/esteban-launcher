use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use esteban_core::channel::{self, Channel};
use esteban_core::signing::{self, Purpose};
use esteban_core::update;
use minisign::{KeyPair, PublicKey, SecretKey, SecretKeyBox};

#[derive(Parser)]
#[command(
    name = "esteban-sign",
    version,
    about = "Make and use the signing keys for Esteban releases, on your own machine. The secret key is only ever written to the file you name."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(
        about = "Make a new keypair: the secret key goes to --out, the public key next to it"
    )]
    Keygen {
        #[arg(
            long,
            value_name = "FILE",
            help = "Where the secret key goes, for example ~/esteban-keys/channel.key"
        )]
        out: PathBuf,
        #[arg(
            long = "for",
            value_enum,
            help = "channel signs versions.json, updater signs latest.json"
        )]
        purpose: Kind,
        #[arg(
            long,
            help = "Leave the secret key without a password (not recommended)"
        )]
        no_password: bool,
    },
    #[command(about = "Sign versions.json or latest.json, writing <file>.minisig next to it")]
    Sign {
        #[arg(long, value_name = "FILE", help = "The secret key from keygen")]
        key: PathBuf,
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    #[command(
        about = "Sign launcher installers and write a signed latest.json for the updater, all next to the installers"
    )]
    Release {
        #[arg(long, value_name = "FILE", help = "The updater secret key from keygen")]
        key: PathBuf,
        #[arg(long, value_name = "X.Y.Z", help = "The version these installers are")]
        version: String,
        #[arg(
            long,
            default_value = "",
            help = "Short release notes shown in the launcher"
        )]
        notes: String,
        #[arg(
            long,
            value_name = "URL",
            help = "Where the files will be downloaded from (default: the launcher's GitHub release for this version)"
        )]
        base_url: Option<String>,
        #[arg(
            value_name = "FILE",
            required = true,
            help = "The AppImage, deb, setup.exe and msi from the release build"
        )]
        files: Vec<PathBuf>,
    },
    #[command(
        about = "Check a signed file against the keys built into the launcher, or a public key file"
    )]
    Verify {
        #[arg(value_name = "FILE")]
        file: PathBuf,
        #[arg(
            long,
            value_name = "FILE",
            help = "A .pub file to check against instead of the built-in keys"
        )]
        public: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Kind {
    Channel,
    Updater,
}

impl Kind {
    fn purpose(self) -> Purpose {
        match self {
            Self::Channel => Purpose::Channel,
            Self::Updater => Purpose::Updater,
        }
    }
}

type Reply<T> = Result<T, String>;

fn kind_of(file: &Path) -> Reply<Purpose> {
    match file.file_name().and_then(|n| n.to_str()) {
        Some("versions.json") => Ok(Purpose::Channel),
        Some("latest.json") => Ok(Purpose::Updater),
        _ => Err("Only versions.json (the Esteban version list) and latest.json (launcher updates) get signed. The file must have one of those names.".into()),
    }
}

fn public_path(secret: &Path) -> PathBuf {
    if secret.extension().and_then(|e| e.to_str()) == Some("key") {
        secret.with_extension("pub")
    } else {
        let mut name = secret.as_os_str().to_owned();
        name.push(".pub");
        PathBuf::from(name)
    }
}

fn signature_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".minisig");
    PathBuf::from(name)
}

fn write_new(path: &Path, bytes: &[u8], private: bool) -> Reply<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = private;
    let mut file = options
        .open(path)
        .map_err(|e| format!("Couldn't create {}: {e}", path.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

fn make_folder(folder: &Path) -> Reply<()> {
    if folder.as_os_str().is_empty() || folder.exists() {
        return Ok(());
    }
    fs::create_dir_all(folder).map_err(|e| format!("Couldn't create {}: {e}", folder.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(folder, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("Couldn't lock down {}: {e}", folder.display()))?;
    }
    Ok(())
}

struct Keygen {
    secret: PathBuf,
    public: PathBuf,
    public_key: String,
    key_id: String,
    encrypted: bool,
}

fn keygen(out: &Path, purpose: Purpose, no_password: bool) -> Reply<Keygen> {
    let public = public_path(out);
    for path in [out, public.as_path()] {
        if path.exists() {
            return Err(format!(
                "{} already exists. Nothing was changed, pick another --out.",
                path.display()
            ));
        }
    }
    if let Some(folder) = out.parent() {
        make_folder(folder)?;
    }
    let pair = if no_password {
        KeyPair::generate_unencrypted_keypair()
    } else {
        KeyPair::generate_encrypted_keypair(None)
    }
    .map_err(|e| format!("Couldn't make the keypair: {e}"))?;
    let comment = format!("esteban-launcher {} secret key", purpose.slug());
    let secret_box = pair
        .sk
        .to_box(Some(&comment))
        .map_err(|e| format!("Couldn't encode the secret key: {e}"))?;
    let public_box = pair
        .pk
        .to_box()
        .map_err(|e| format!("Couldn't encode the public key: {e}"))?;
    write_new(out, &secret_box.to_bytes(), true)?;
    write_new(&public, &public_box.to_bytes(), false)?;
    let public_key = pair.pk.to_base64();
    Ok(Keygen {
        secret: out.to_path_buf(),
        public,
        key_id: signing::key_id(&public_key).unwrap_or_default(),
        public_key,
        encrypted: !no_password,
    })
}

fn load_secret(key: &Path) -> Reply<SecretKey> {
    let text =
        fs::read_to_string(key).map_err(|e| format!("Couldn't read {}: {e}", key.display()))?;
    let parse = || {
        SecretKeyBox::from_string(&text)
            .map_err(|_| format!("{} isn't a secret key from keygen.", key.display()))
    };
    if let Ok(plain) = parse()?.into_unencrypted_secret_key() {
        return Ok(plain);
    }
    parse()?
        .into_secret_key(None)
        .map_err(|_| "Wrong password, or the key file is damaged. Nothing was signed.".to_string())
}

fn describe(purpose: Purpose, bytes: &[u8], now: &str) -> Reply<String> {
    match purpose {
        Purpose::Channel => {
            let list = Channel::parse(bytes).map_err(|e| format!("Not signed: {e}"))?;
            if list.expires.as_str() <= now {
                return Err(format!(
                    "Not signed: it expires {}, which has already passed. Move expires forward.",
                    list.expires
                ));
            }
            Ok(format!("sequence:{}", list.sequence))
        }
        Purpose::Updater => {
            let manifest = update::parse(bytes).map_err(|e| format!("Not signed: {e}"))?;
            Ok(format!("version:{}", manifest.version))
        }
    }
}

#[derive(Debug)]
struct Signed {
    signature: PathBuf,
    key_id: String,
    trusted_comment: String,
    built_in: bool,
}

fn sign(key: &Path, file: &Path) -> Reply<Signed> {
    let purpose = kind_of(file)?;
    let bytes = fs::read(file).map_err(|e| format!("Couldn't read {}: {e}", file.display()))?;
    let summary = describe(purpose, &bytes, &channel::utc_now())?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let trusted_comment = format!("esteban-launcher {} {name} {summary}", purpose.slug());
    let secret = load_secret(key)?;
    let public = PublicKey::from_secret_key(&secret)
        .map_err(|e| format!("Couldn't read the public half of the key: {e}"))?;
    let signature = minisign::sign(
        Some(&public),
        &secret,
        Cursor::new(&bytes),
        Some(&trusted_comment),
        Some("signature from esteban-sign"),
    )
    .map_err(|e| format!("Signing failed: {e}"))?
    .into_string();
    let public_key = public.to_base64();
    let verified = signing::verify_with(
        std::slice::from_ref(&public_key),
        purpose,
        &bytes,
        &signature,
    )
    .map_err(|e| format!("The new signature didn't check out, nothing was written: {e}"))?;
    let target = signature_path(file);
    let staging = target.with_extension("minisig.part");
    if staging.exists() {
        fs::remove_file(&staging)
            .map_err(|e| format!("Couldn't clear {}: {e}", staging.display()))?;
    }
    write_new(&staging, signature.as_bytes(), false)?;
    fs::rename(&staging, &target)
        .map_err(|e| format!("Couldn't write {}: {e}", target.display()))?;
    Ok(Signed {
        signature: target,
        key_id: verified.key_id,
        trusted_comment: verified.trusted_comment,
        built_in: signing::built_in(purpose).contains(&public_key),
    })
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn platform_of(name: &str) -> Option<&'static str> {
    if name.ends_with(".AppImage") {
        Some("linux-x86_64-appimage")
    } else if name.ends_with(".deb") {
        Some("linux-x86_64-deb")
    } else if name.ends_with("-setup.exe") {
        Some("windows-x86_64-nsis")
    } else if name.ends_with(".msi") {
        Some("windows-x86_64-msi")
    } else {
        None
    }
}

fn minisign_text(
    secret: &SecretKey,
    public: &PublicKey,
    bytes: &[u8],
    trusted_comment: &str,
) -> Reply<String> {
    Ok(minisign::sign(
        Some(public),
        secret,
        Cursor::new(bytes),
        Some(trusted_comment),
        Some("signature from esteban-sign"),
    )
    .map_err(|e| format!("Signing failed: {e}"))?
    .into_string())
}

fn write_replacing(path: &Path, bytes: &[u8]) -> Reply<()> {
    let mut staging = path.as_os_str().to_owned();
    staging.push(".part");
    let staging = PathBuf::from(staging);
    if staging.exists() {
        fs::remove_file(&staging)
            .map_err(|e| format!("Couldn't clear {}: {e}", staging.display()))?;
    }
    write_new(&staging, bytes, false)?;
    fs::rename(&staging, path).map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

#[derive(Debug)]
struct Released {
    written: Vec<PathBuf>,
    key_id: String,
    built_in: bool,
}

fn release(
    key: &Path,
    version: &str,
    notes: &str,
    base_url: Option<&str>,
    files: &[PathBuf],
) -> Reply<Released> {
    if esteban_core::system::parse_version(version).is_none_or(|n| n.len() != 3) {
        return Err(format!("{version} isn't a version like 0.2.0."));
    }
    let base = base_url
        .map(|b| b.trim_end_matches('/').to_string())
        .unwrap_or_else(|| {
            format!("https://github.com/SunqdXX/esteban-launcher/releases/download/v{version}")
        });
    let folder = files
        .first()
        .and_then(|f| f.parent())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let mut entries = serde_json::Map::new();
    let mut items = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("{} has no usable file name.", file.display()))?
            .to_string();
        let platform = platform_of(&name).ok_or_else(|| {
            format!("{name} isn't an AppImage, deb, setup.exe or msi from the release build.")
        })?;
        if name.contains(char::is_whitespace) || !name.contains(version) {
            return Err(format!(
                "{name} should be named the way the release build names it, with {version} in it and no spaces."
            ));
        }
        if file.parent().map(Path::to_path_buf).unwrap_or_default() != folder {
            return Err("Put all the installers in one folder first.".into());
        }
        if entries.contains_key(platform) {
            return Err(format!("Two files are for {platform}. Pass one of each."));
        }
        entries.insert(platform.to_string(), serde_json::Value::Null);
        items.push((file.clone(), name, platform));
    }
    let secret = load_secret(key)?;
    let public = PublicKey::from_secret_key(&secret)
        .map_err(|e| format!("Couldn't read the public half of the key: {e}"))?;
    let public_key = public.to_base64();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut written = Vec::new();
    for (file, name, platform) in &items {
        let bytes = fs::read(file).map_err(|e| format!("Couldn't read {}: {e}", file.display()))?;
        let comment = format!("timestamp:{stamp}\tfile:{name}\tversion:{version}");
        let text = minisign_text(&secret, &public, &bytes, &comment)?;
        let encoded = base64_encode(text.as_bytes());
        check_installer_signature(&public_key, &bytes, &encoded, version)?;
        let sig_path = {
            let mut p = file.as_os_str().to_owned();
            p.push(".sig");
            PathBuf::from(p)
        };
        write_replacing(&sig_path, encoded.as_bytes())?;
        written.push(sig_path);
        entries.insert(
            (*platform).to_string(),
            serde_json::json!({ "signature": encoded, "url": format!("{base}/{name}") }),
        );
    }
    let manifest = serde_json::json!({
        "version": version,
        "notes": notes,
        "pub_date": channel::utc_now(),
        "platforms": entries,
    });
    let latest = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| format!("Couldn't write latest.json: {e}"))?;
    update::parse(&latest).map_err(|e| format!("latest.json came out wrong: {e}"))?;
    let latest_path = folder.join("latest.json");
    let latest_comment = format!("esteban-launcher updater latest.json version:{version}");
    let latest_signature = minisign_text(&secret, &public, &latest, &latest_comment)?;
    let verified = signing::verify_with(
        std::slice::from_ref(&public_key),
        Purpose::Updater,
        &latest,
        &latest_signature,
    )
    .map_err(|e| format!("The latest.json signature didn't check out, nothing was written: {e}"))?;
    write_replacing(&latest_path, &latest)?;
    written.push(latest_path.clone());
    let latest_sig_path = signature_path(&latest_path);
    write_replacing(&latest_sig_path, latest_signature.as_bytes())?;
    written.push(latest_sig_path);
    Ok(Released {
        written,
        key_id: verified.key_id,
        built_in: signing::built_in(Purpose::Updater).contains(&public_key),
    })
}

fn check_installer_signature(
    public_key: &str,
    bytes: &[u8],
    encoded: &str,
    version: &str,
) -> Reply<()> {
    let text = esteban_core::signing::decode_base64_text(encoded)
        .ok_or("The installer signature didn't encode properly.")?;
    let verified = signing::verify_with(&[public_key.to_string()], Purpose::Updater, bytes, &text)
        .map_err(|e| {
            format!("An installer signature didn't check out, nothing was written: {e}")
        })?;
    let signed_version = verified
        .trusted_comment
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"));
    if signed_version != Some(version) {
        return Err(
            "An installer signature doesn't carry the version, nothing was written.".into(),
        );
    }
    Ok(())
}

fn verify(file: &Path, public: Option<&Path>) -> Reply<String> {
    let purpose = kind_of(file)?;
    let bytes = fs::read(file).map_err(|e| format!("Couldn't read {}: {e}", file.display()))?;
    let signature_file = signature_path(file);
    let signature = fs::read_to_string(&signature_file)
        .map_err(|e| format!("Couldn't read {}: {e}", signature_file.display()))?;
    let keys = match public {
        Some(path) => vec![
            PublicKey::from_file(path)
                .map_err(|_| format!("{} isn't a public key from keygen.", path.display()))?
                .to_base64(),
        ],
        None => signing::built_in(purpose).to_vec(),
    };
    let verified =
        signing::verify_with(&keys, purpose, &bytes, &signature).map_err(|e| e.to_string())?;
    describe(purpose, &bytes, &channel::utc_now())?;
    Ok(format!(
        "Good signature from key {}. Signed: {}",
        verified.key_id, verified.trusted_comment
    ))
}

fn run(cli: Cli) -> Reply<()> {
    match cli.command {
        Command::Keygen {
            out,
            purpose,
            no_password,
        } => {
            let made = keygen(&out, purpose.purpose(), no_password)?;
            println!();
            println!(
                "Secret key: {} ({})",
                made.secret.display(),
                if made.encrypted {
                    "encrypted with your password"
                } else {
                    "NOT encrypted"
                }
            );
            println!("Public key: {}", made.public.display());
            println!();
            println!("{}", made.public_key);
            println!();
            println!(
                "That line is the public key (id {}). It's safe to share: it goes into config/keys.json under \"{}\".",
                made.key_id,
                purpose.purpose().slug()
            );
            println!(
                "Keep the secret key offline and backed up. Anyone holding it (and its password) can sign releases the launcher trusts."
            );
        }
        Command::Sign { key, file } => {
            let signed = sign(&key, &file)?;
            println!(
                "Wrote {} (key {}).",
                signed.signature.display(),
                signed.key_id
            );
            println!("Signed: {}", signed.trusted_comment);
            if !signed.built_in {
                println!(
                    "This key isn't in config/keys.json yet, so this build of the launcher won't trust it."
                );
            }
        }
        Command::Release {
            key,
            version,
            notes,
            base_url,
            files,
        } => {
            let done = release(&key, &version, &notes, base_url.as_deref(), &files)?;
            println!("Signed with key {}. Wrote:", done.key_id);
            for path in &done.written {
                println!("  {}", path.display());
            }
            if !done.built_in {
                println!(
                    "This key isn't the updater key in config/keys.json, so the launcher won't install these."
                );
            }
            println!("Upload every file listed above to the draft release, then publish it.");
        }
        Command::Verify { file, public } => {
            println!("{}", verify(&file, public.as_deref())?);
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = include_str!("../../../config/versions.json");

    #[test]
    fn keygen_writes_both_files_once_and_keeps_the_secret_private() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("keys").join("channel.key");
        let made = keygen(&out, Purpose::Channel, true).unwrap();
        assert_eq!(made.public, dir.path().join("keys").join("channel.pub"));
        let secret_text = fs::read_to_string(&out).unwrap();
        let public_text = fs::read_to_string(&made.public).unwrap();
        assert!(public_text.contains(&made.public_key));
        assert!(!secret_text.contains(&made.public_key));
        assert_eq!(made.key_id.len(), 16);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&out).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
            let folder = fs::metadata(out.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(folder, 0o700);
        }
        let again = keygen(&out, Purpose::Channel, true);
        assert!(again.is_err());
        assert_eq!(fs::read_to_string(&out).unwrap(), secret_text);
    }

    #[test]
    fn a_signed_list_verifies_with_its_key_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("channel.key");
        let made = keygen(&key, Purpose::Channel, true).unwrap();
        let list = dir.path().join("versions.json");
        fs::write(&list, LIST).unwrap();
        let signed = sign(&key, &list).unwrap();
        assert_eq!(signed.signature, dir.path().join("versions.json.minisig"));
        assert!(
            signed
                .trusted_comment
                .contains("channel versions.json sequence:")
        );
        assert!(!signed.built_in);
        assert!(
            verify(&list, Some(&made.public))
                .unwrap()
                .starts_with("Good signature")
        );

        let other = dir.path().join("other.key");
        let stranger = keygen(&other, Purpose::Channel, true).unwrap();
        assert!(verify(&list, Some(&stranger.public)).is_err());
        fs::write(&list, LIST.replace("\"sequence\": 1", "\"sequence\": 2")).unwrap();
        assert!(verify(&list, Some(&made.public)).is_err());
    }

    #[test]
    fn broken_lists_and_unknown_files_are_never_signed() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("channel.key");
        keygen(&key, Purpose::Channel, true).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(LIST).unwrap();
        let hacks = value["builds"][1]["jars"][1].clone();
        value["builds"][0]["jars"]
            .as_array_mut()
            .unwrap()
            .push(hacks);
        let list = dir.path().join("versions.json");
        fs::write(&list, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            sign(&key, &list)
                .unwrap_err()
                .contains("puts the hacks mod")
        );
        assert!(!dir.path().join("versions.json.minisig").exists());

        let mut stale: serde_json::Value = serde_json::from_str(LIST).unwrap();
        stale["issued"] = "2020-01-01T00:00:00Z".into();
        stale["expires"] = "2020-02-01T00:00:00Z".into();
        fs::write(&list, serde_json::to_vec(&stale).unwrap()).unwrap();
        assert!(sign(&key, &list).unwrap_err().contains("already passed"));

        let other = dir.path().join("notes.json");
        fs::write(&other, "{}").unwrap();
        assert!(sign(&key, &other).is_err());
        assert!(sign(&dir.path().join("channel.pub"), &list).is_err());
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"a"), "YQ==");
        assert_eq!(base64_encode(b"ab"), "YWI=");
        assert_eq!(base64_encode(b"abc"), "YWJj");
        assert_eq!(
            base64_encode(b"any carnal pleas"),
            "YW55IGNhcm5hbCBwbGVhcw=="
        );
        let text = "untrusted comment: x\nRWQ+/=\n";
        assert_eq!(
            esteban_core::signing::decode_base64_text(&base64_encode(text.as_bytes())).as_deref(),
            Some(text)
        );
    }

    fn installers(dir: &Path, version: &str) -> Vec<PathBuf> {
        [
            format!("esteban-launcher-{version}-linux-x86_64.AppImage"),
            format!("esteban-launcher-{version}-linux-amd64.deb"),
            format!("esteban-launcher-{version}-windows-x64-setup.exe"),
            format!("esteban-launcher-{version}-windows-x64.msi"),
        ]
        .iter()
        .map(|name| {
            let path = dir.join(name);
            fs::write(&path, format!("installer bytes for {name}")).unwrap();
            path
        })
        .collect()
    }

    #[test]
    fn a_release_is_signed_the_way_tauris_updater_checks_it() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("updater.key");
        let made = keygen(&key, Purpose::Updater, true).unwrap();
        let out = dir.path().join("release");
        fs::create_dir_all(&out).unwrap();
        let files = installers(&out, "0.2.0");
        let done = release(&key, "0.2.0", "Faster installs", None, &files).unwrap();
        assert_eq!(done.written.len(), 6);
        assert!(!done.built_in);

        let tauri_pubkey = base64_encode(fs::read_to_string(&made.public).unwrap().as_bytes());
        let pubkey_text = esteban_core::signing::decode_base64_text(&tauri_pubkey).unwrap();
        let public = minisign_verify::PublicKey::decode(&pubkey_text).unwrap();
        let latest = fs::read(out.join("latest.json")).unwrap();
        let manifest = update::parse(&latest).unwrap();
        assert_eq!(manifest.version, "0.2.0");
        assert_eq!(manifest.platforms.len(), 4);
        for file in &files {
            let name = file.file_name().unwrap().to_str().unwrap();
            let entry = manifest
                .platforms
                .values()
                .find(|e| e.url.ends_with(name))
                .unwrap();
            assert_eq!(
                entry.url,
                format!(
                    "https://github.com/SunqdXX/esteban-launcher/releases/download/v0.2.0/{name}"
                )
            );
            let mut sig_name = file.as_os_str().to_owned();
            sig_name.push(".sig");
            assert_eq!(fs::read_to_string(sig_name).unwrap(), entry.signature);
            let text = esteban_core::signing::decode_base64_text(&entry.signature).unwrap();
            let signature = minisign_verify::Signature::decode(&text).unwrap();
            public
                .verify(&fs::read(file).unwrap(), &signature, true)
                .unwrap();
            assert!(
                signature
                    .trusted_comment()
                    .split('\t')
                    .any(|f| f == "version:0.2.0")
            );
            assert!(public.verify(b"tampered", &signature, true).is_err());
        }
        let latest_sig = fs::read_to_string(out.join("latest.json.minisig")).unwrap();
        assert!(
            signing::verify_with(
                std::slice::from_ref(&made.public_key),
                Purpose::Updater,
                &latest,
                &latest_sig
            )
            .is_ok()
        );
    }

    #[test]
    fn releases_with_wrong_names_versions_or_duplicates_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("updater.key");
        keygen(&key, Purpose::Updater, true).unwrap();
        let files = installers(dir.path(), "0.2.0");
        assert!(
            release(&key, "0.3.0", "", None, &files)
                .unwrap_err()
                .contains("with 0.3.0 in it")
        );
        assert!(release(&key, "soon", "", None, &files).is_err());
        let doubled = vec![files[0].clone(), files[0].clone()];
        assert!(
            release(&key, "0.2.0", "", None, &doubled)
                .unwrap_err()
                .contains("Two files")
        );
        let spaced = dir.path().join("Esteban Launcher_0.2.0_amd64.AppImage");
        fs::write(&spaced, "x").unwrap();
        assert!(release(&key, "0.2.0", "", None, &[spaced]).is_err());
        let other = dir.path().join("esteban-launcher-0.2.0.zip");
        fs::write(&other, "x").unwrap();
        assert!(release(&key, "0.2.0", "", None, &[other]).is_err());
        assert!(!dir.path().join("latest.json").exists());
    }
}
