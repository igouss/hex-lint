//! Driven adapter: read the exceptions TOML file.
//!
//! File format:
//!
//! ```toml
//! [[exception]]
//! consumer = "my-domain"
//! dep      = "legacy-helper"
//! ticket   = "ARCH-1234"
//! reason   = "deleting after the rewrite lands"
//! ```

use std::io;
use std::path::Path;

use serde::Deserialize;

use crate::lint::Exception;

#[derive(Deserialize)]
struct ExceptionsFile {
    #[serde(rename = "exception", default)]
    exceptions: Vec<Exception>,
}

#[derive(Debug)]
pub enum LoadError {
    NotFound,
    Io(String),
    Parse(String),
}

/// Read and parse the exceptions file at `path`. Returns `LoadError::NotFound`
/// if the file does not exist (caller decides whether that's fatal).
pub fn load(path: &Path) -> Result<Vec<Exception>, LoadError> {
    let raw: String = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(LoadError::NotFound),
        Err(e) => return Err(LoadError::Io(e.to_string())),
    };
    parse(&raw).map_err(LoadError::Parse)
}

fn parse(raw: &str) -> Result<Vec<Exception>, String> {
    toml::from_str::<ExceptionsFile>(raw)
        .map(|f| f.exceptions)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::lint::Exception;

    #[test]
    fn empty_file_yields_no_exceptions() {
        let parsed: Vec<Exception> = parse("").expect("empty TOML is valid");
        assert!(parsed.is_empty());
    }

    #[test]
    fn parses_well_formed_exceptions() {
        let raw: &str = r#"
[[exception]]
consumer = "my-domain"
dep      = "legacy-helper"
ticket   = "ARCH-1234"
reason   = "deleting after the rewrite lands"

[[exception]]
consumer = "other"
dep      = "thing"
ticket   = "ARCH-5"
reason   = "tbd"
"#;
        let parsed: Vec<Exception> = parse(raw).expect("valid TOML");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].consumer, "my-domain");
        assert_eq!(parsed[0].dep, "legacy-helper");
        assert_eq!(parsed[0].ticket, "ARCH-1234");
        assert_eq!(parsed[1].ticket, "ARCH-5");
    }

    #[test]
    fn missing_field_is_an_error() {
        let raw: &str = r#"
[[exception]]
consumer = "my-domain"
dep      = "legacy-helper"
"#;
        assert!(parse(raw).is_err(), "missing ticket+reason should fail");
    }

    #[test]
    fn malformed_toml_is_an_error() {
        assert!(parse("[[exception]\nbroken").is_err());
    }
}
