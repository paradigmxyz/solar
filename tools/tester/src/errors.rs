//! <https://github.com/rust-lang/rust/blob/af44e719fa16832425c0764ac9c54ad82a617d3a/src/tools/compiletest/src/errors.rs>

use crate::solc::SolcErrorKind;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{fmt, str::FromStr};

use self::WhichLine::*;

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
pub struct Error {
    pub line_num: usize,
    /// What kind of message we expect (e.g., warning, error, suggestion).
    /// `None` if not specified or unknown message kind.
    pub kind: Option<ErrorKind>,
    pub msg: String,
    pub solc_kind: Option<SolcErrorKind>,
}

impl Error {
    /// Looks for either "//~| KIND MESSAGE" or "//~^^... KIND MESSAGE"
    /// The former is a "follow" that inherits its target from the preceding line;
    /// the latter is an "adjusts" that goes that many lines up.
    ///
    /// Goal is to enable tests both like: //~^^^ ERROR go up three
    /// and also //~^ ERROR message one for the preceding line, and
    ///          //~| ERROR message two for that same line.
    ///
    /// If cfg is not None (i.e., in an incremental test), then we look
    /// for `//[X]~` instead, where `X` is the current `cfg`.
    pub fn load(from: impl Iterator<Item = impl AsRef<str>>, cfg: Option<&str>) -> Vec<Self> {
        // `last_nonfollow_error` tracks the most recently seen
        // line with an error template that did not use the
        // follow-syntax, "//~| ...".
        //
        // (pnkfelix could not find an easy way to compose Iterator::scan
        // and Iterator::filter_map to pass along this information into
        // `parse_expected`. So instead I am storing that state here and
        // updating it in the map callback below.)
        let mut last_nonfollow_error = None;

        from.enumerate()
            .filter_map(|(line_num, line)| {
                parse_expected(last_nonfollow_error, line_num + 1, line.as_ref(), cfg).map(
                    |(which, error)| {
                        match which {
                            FollowPrevious(_) => {}
                            _ => last_nonfollow_error = Some(error.line_num),
                        }

                        error
                    },
                )
            })
            .collect()
    }

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

    pub(crate) fn is_error(&self) -> bool {
        matches!(self.kind, Some(ErrorKind::Error))
    }
}

#[derive(PartialEq, Debug)]
enum WhichLine {
    ThisLine,
    FollowPrevious(usize),
    AdjustBackward(usize),
}

fn parse_expected(
    last_nonfollow_error: Option<usize>,
    line_num: usize,
    line: &str,
    cfg: Option<&str>,
) -> Option<(WhichLine, Error)> {
    // Matches comments like:
    //     //~
    //     //~|
    //     //~^
    //     //~^^^^^
    //     //[cfg1]~
    //     //[cfg1,cfg2]~^^
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"//(?:\[(?P<cfgs>[\w,]+)])?~(?P<adjust>\||\^*)").unwrap());

    let captures = RE.captures(line)?;

    match (cfg, captures.name("cfgs")) {
        // Only error messages that contain our `cfg` between the square brackets apply to us.
        (Some(cfg), Some(filter)) if !filter.as_str().split(',').any(|s| s == cfg) => return None,
        (Some(_), Some(_)) => {}

        (None, Some(_)) => panic!("Only tests with revisions should use `//[X]~`"),

        // If an error has no list of revisions, it applies to all revisions.
        (Some(_), None) | (None, None) => {}
    }

    let (follow, adjusts) = match &captures["adjust"] {
        "|" => (true, 0),
        circumflexes => (false, circumflexes.len()),
    };

    // Get the part of the comment after the sigil (e.g. `~^^` or ~|).
    let whole_match = captures.get(0).unwrap();
    let (_, mut msg) = line.split_at(whole_match.end());

    let first_word = msg.split_whitespace().next().expect("Encountered unexpected empty comment");

    // If we find `//~ ERROR foo` or something like that, skip the first word.
    let kind = first_word.parse::<ErrorKind>().ok();
    if kind.is_some() {
        msg = &msg.trim_start().split_at(first_word.len()).1;
    }

    let msg = msg.trim().to_owned();

    let (which, line_num) = if follow {
        assert_eq!(adjusts, 0, "use either //~| or //~^, not both.");
        let line_num = last_nonfollow_error.expect(
            "encountered //~| without \
             preceding //~^ line.",
        );
        (FollowPrevious(line_num), line_num)
    } else {
        let which = if adjusts > 0 { AdjustBackward(adjusts) } else { ThisLine };
        let line_num = line_num - adjusts;
        (which, line_num)
    };

    // debug!(
    //     "line={} tag={:?} which={:?} kind={:?} msg={:?}",
    //     line_num,
    //     whole_match.as_str(),
    //     which,
    //     kind,
    //     msg
    // );
    Some((which, Error { line_num, kind, msg, solc_kind: None }))
}
