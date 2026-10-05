use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OsName {
    Windows,
    Linux,
    Osx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
    X86,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Platform {
    pub os: OsName,
    pub arch: Arch,
    pub os_version: Option<Vec<u64>>,
}

impl Platform {
    pub fn current() -> Result<Self> {
        let os = match std::env::consts::OS {
            "windows" => OsName::Windows,
            "linux" => OsName::Linux,
            "macos" => OsName::Osx,
            other => return Err(Error::Unsupported(format!("{other} is not supported"))),
        };
        let arch = match std::env::consts::ARCH {
            "x86_64" => Arch::X86_64,
            "aarch64" => Arch::Aarch64,
            "x86" => Arch::X86,
            _ => Arch::Other,
        };
        Ok(Self {
            os,
            arch,
            os_version: None,
        })
    }

    pub fn mojang_os(&self) -> &'static str {
        match self.os {
            OsName::Windows => "windows",
            OsName::Linux => "linux",
            OsName::Osx => "osx",
        }
    }

    pub fn runtime_key(&self) -> Result<&'static str> {
        match (self.os, self.arch) {
            (OsName::Linux, Arch::X86_64) => Ok("linux"),
            (OsName::Windows, Arch::X86_64) => Ok("windows-x64"),
            (OsName::Windows, Arch::Aarch64) => Ok("windows-arm64"),
            (OsName::Osx, Arch::X86_64) => Ok("mac-os"),
            (OsName::Osx, Arch::Aarch64) => Ok("mac-os-arm64"),
            (OsName::Linux, Arch::Aarch64) => Err(Error::Unsupported(
                "Mojang does not publish Java for Linux on ARM, so this machine is not supported yet".into(),
            )),
            _ => Err(Error::Unsupported("this CPU architecture is not supported".into())),
        }
    }

    pub fn classpath_separator(&self) -> &'static str {
        match self.os {
            OsName::Windows => ";",
            OsName::Linux | OsName::Osx => ":",
        }
    }

    pub fn java_binary(&self) -> &'static str {
        match self.os {
            OsName::Windows => "bin/java.exe",
            OsName::Linux => "bin/java",
            OsName::Osx => "jre.bundle/Contents/Home/bin/java",
        }
    }
}

pub fn parse_version(text: &str) -> Option<Vec<u64>> {
    text.split('.').map(|part| part.parse().ok()).collect()
}

pub fn total_memory_bytes() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.total_memory()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_keys_follow_mojangs_platform_names() {
        let p = |os, arch| Platform {
            os,
            arch,
            os_version: None,
        };
        assert_eq!(
            p(OsName::Linux, Arch::X86_64).runtime_key().unwrap(),
            "linux"
        );
        assert_eq!(
            p(OsName::Windows, Arch::X86_64).runtime_key().unwrap(),
            "windows-x64"
        );
        assert!(p(OsName::Linux, Arch::Aarch64).runtime_key().is_err());
    }

    #[test]
    fn versions_parse_as_numbers() {
        assert_eq!(parse_version("10.0.17134"), Some(vec![10, 0, 17134]));
        assert_eq!(parse_version("10.x"), None);
    }
}
