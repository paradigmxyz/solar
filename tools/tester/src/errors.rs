//! <https://github.com/rust-lang/rust/blob/af44e719fa16832425c0764ac9c54ad82a617d3a/src/tools/compiletest/src/errors.rs>

use crate::solc::SolcErrorKind;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{fmt, str::FromStr};

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ErrorKind {
    Help,
    Error,
    Note,
    Suggestion,
    Warning,
}

impl FromStr for ErrorKind {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().split(':').next().unwrap() {
            "HELP" => Ok(Self::Help),
            "ERROR" => Ok(Self::Error),
            "NOTE" => Ok(Self::Note),
            "SUGGESTION" => Ok(Self::Suggestion),
            "WARN" | "WARNING" => Ok(Self::Warning),
            _ => Err(()),
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Help => write!(f, "help message"),
            Self::Error => write!(f, "error"),
            Self::Note => write!(f, "note"),
            Self::Suggestion => write!(f, "suggestion"),
            Self::Warning => write!(f, "warning"),
        }
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Error {
    pub line_num: usize,
    /// What kind of message we expect (e.g., warning, error, suggestion).
    /// `None` if not specified or unknown message kind.
    pub kind: Option<ErrorKind>,
    pub msg: String,
    pub solc_kind: Option<SolcErrorKind>,
}

impl Error {
    pub fn load_solc(file: &str) -> Vec<Self> {
        const DELIM: &str = "// ----";
        // Warning 2519: (80-89): This declaration shadows an existing declaration.
        static ERROR_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"//\s*(\w+)\s*(?:\d+)?:\s*(?:\((\d+)-(\d+)\):)?(.*)").unwrap()
        });

        let mut errors = Vec::new();
        if let Some(idx) = file.rfind(DELIM) {
            let re = &*ERROR_RE;
            for line in file[idx..].lines().skip(1) {
                if let Some(caps) = re.captures(line) {
                    if let Ok(solc_kind) = caps.get(1).unwrap().as_str().parse::<SolcErrorKind>() {
                        let error_kind = match solc_kind {
                            SolcErrorKind::Info => ErrorKind::Note,
                            SolcErrorKind::Warning => ErrorKind::Warning,
                            _ => ErrorKind::Error,
                        };
                        let start =
                            caps.get(2).map(|cap| cap.as_str().parse().unwrap()).unwrap_or(0);
                        let line_num = file[..start].lines().count();
                        let msg = caps.get(4).unwrap().as_str().trim().to_owned();
                        errors.push(Self {
                            line_num,
                            kind: Some(error_kind),
                            msg,
                            solc_kind: Some(solc_kind),
                        });
                    }
                }
            }
        }
        errors
    }

    pub fn is_error(&self) -> bool {
        matches!(self.kind, Some(ErrorKind::Error))
    }
}
