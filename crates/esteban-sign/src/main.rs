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
}
