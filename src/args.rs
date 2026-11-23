use std::path::PathBuf;

use crate::role::Role;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Format {
    Text,
    Json,
}

pub struct Args {
    pub exceptions: Option<PathBuf>,
    pub manifest_path: Option<PathBuf>,
    pub format: Format,
}

#[cfg_attr(test, derive(Debug))]
pub enum ParseOutcome {
    Run(Args),
    Explain(Role),
    CapabilitiesRequested,
    HelpRequested,
    VersionRequested,
}

#[cfg(test)]
impl std::fmt::Debug for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Args")
            .field("exceptions", &self.exceptions)
            .field("manifest_path", &self.manifest_path)
            .field("format", &self.format.as_str())
            .finish()
    }
}

#[cfg(test)]
impl std::fmt::Debug for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Format {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

#[cfg(test)]
impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
        }
    }
}

pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<ParseOutcome, String> {
    let mut it = args.into_iter();
    let mut exceptions: Option<PathBuf> = None;
    let mut manifest_path: Option<PathBuf> = None;
    let mut format: Format = Format::Text;

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParseOutcome::HelpRequested),
            "-V" | "--version" => return Ok(ParseOutcome::VersionRequested),
            "--capabilities" => return Ok(ParseOutcome::CapabilitiesRequested),
            "explain" => {
                let v: String = it
                    .next()
                    .ok_or_else(|| "explain requires a role name".to_owned())?;
                let role: Role = Role::parse(&v).ok_or_else(|| {
                    format!(
                        "unknown role `{v}` (expected one of: {})",
                        Role::all_names()
                    )
                })?;
                return Ok(ParseOutcome::Explain(role));
            }
            "-e" | "--exceptions" => {
                let v: String = it
                    .next()
                    .ok_or_else(|| "--exceptions requires a value".to_owned())?;
                exceptions = Some(PathBuf::from(v));
            }
            "--manifest-path" => {
                let v: String = it
                    .next()
                    .ok_or_else(|| "--manifest-path requires a value".to_owned())?;
                manifest_path = Some(PathBuf::from(v));
            }
            "-f" | "--format" => {
                let v: String = it
                    .next()
                    .ok_or_else(|| "--format requires a value".to_owned())?;
                format =
                    Format::parse(&v).ok_or_else(|| format!("unknown format `{v}` (text|json)"))?;
            }
            other => {
                if let Some(v) = other.strip_prefix("--exceptions=") {
                    exceptions = Some(PathBuf::from(v));
                } else if let Some(v) = other.strip_prefix("--manifest-path=") {
                    manifest_path = Some(PathBuf::from(v));
                } else if let Some(v) = other.strip_prefix("--format=") {
                    format = Format::parse(v)
                        .ok_or_else(|| format!("unknown format `{v}` (text|json)"))?;
                } else {
                    return Err(format!("unknown argument: {other}"));
                }
            }
        }
    }
    Ok(ParseOutcome::Run(Args {
        exceptions,
        manifest_path,
        format,
    }))
}
