//! Recorded rewrites and scripted candidates for `llm-optimize`.
//!
//! The cache is a directory with one file per rewrite, named by a hash of everything a rewrite
//! depends on: the format version, the target EVM version, the objective, the optimizer runs, and
//! the original's candidate text. A file holds the original, the rewrite, and a note of where the
//! rewrite came from:
//!
//! ```text
//! solar llm-optimize cache v1
//! --- original
//! fn @f(arg0: i256) -> i256 {
//!   ...
//! }
//! --- rewrite
//! fn @f(arg0: i256) -> i256 {
//!   ...
//! }
//! --- evidence
//! source: script
//! samples: 512
//! ```
//!
//! An entry is used only when its original matches the function exactly, and its rewrite goes
//! through every check again, so a stale or edited cache cannot change code the checks reject.
//! Entries hold the MIR of the code they rewrite, so on Unix only their owner may read them. Each
//! is written to a fresh temporary file and renamed into place.
//!
//! A script lists the candidates a scripted rewriter proposes for each function, in order, after
//! any preamble:
//!
//! ```text
//! --- function f
//! --- candidate
//! fn @f(arg0: i256) -> i256 {
//!   ...
//! }
//! ```

use crate::{
    llm::{LlmError, LlmRewriter, LlmSession, Proposal, RewriteRequest, Verdict},
    target::Target,
};
use alloy_primitives::{hex, keccak256};
use solar_data_structures::map::FxHashMap;
use std::{
    fs::{File, OpenOptions},
    hash::{BuildHasher, Hasher, RandomState},
    io::{self, Write},
    path::{Path, PathBuf},
};

/// First line of every cache file; changing the format or the checks bumps it.
const CACHE_VERSION: &str = "solar llm-optimize cache v1";

/// A directory of recorded rewrites.
pub(super) struct Cache {
    dir: PathBuf,
}

/// One recorded rewrite.
pub(super) struct Entry {
    /// The original's candidate text.
    pub(super) original: String,
    /// The rewrite's candidate text.
    pub(super) rewrite: String,
    /// Where the rewrite came from.
    pub(super) evidence: String,
}

impl Cache {
    pub(super) fn new(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf() }
    }

    /// Returns the file name of the entry for `original` under `target`.
    pub(super) fn key(target: Target, original: &str) -> String {
        let key = format!(
            "{CACHE_VERSION}\n{}\n{}\n{}\n{original}",
            target.evm_version(),
            target.optimization(),
            target.expected_executions()
        );
        format!("{}.llm", hex::encode(keccak256(key)))
    }

    /// Loads the entry named `key`: `Ok(None)` when there is none.
    pub(super) fn load(&self, key: &str) -> Result<Option<Entry>, String> {
        // Cache files are compiler state, not sources, so they bypass the source loader.
        #[allow(clippy::disallowed_methods)]
        let text = match std::fs::read_to_string(self.dir.join(key)) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("cannot read cache entry `{key}`: {error}")),
        };
        let Some(body) = text.strip_prefix(CACHE_VERSION).and_then(|body| body.strip_prefix('\n'))
        else {
            return Err(format!("cache entry `{key}` is not `{CACHE_VERSION}`"));
        };
        let sections = sections(body)?;
        let mut original = None;
        let mut rewrite = None;
        let mut evidence = String::new();
        for (marker, text) in sections {
            match marker.as_str() {
                "original" => original = Some(text),
                "rewrite" => rewrite = Some(text),
                "evidence" => evidence = text,
                _ => return Err(format!("cache entry `{key}` has a `--- {marker}` section")),
            }
        }
        let (Some(original), Some(rewrite)) = (original, rewrite) else {
            return Err(format!("cache entry `{key}` lacks its original or its rewrite"));
        };
        Ok(Some(Entry { original, rewrite, evidence }))
    }

    /// Records `entry` as `key`.
    ///
    /// An entry holds the original's MIR, as private as the sources it comes from, so on Unix
    /// the files are their owner's alone, and so is a directory the cache creates. The entry is
    /// written to a temporary file of a fresh random name, created only where nothing exists,
    /// so that no file or link another user prepares there receives it, and renamed into place.
    pub(super) fn store(&self, key: &str, entry: &Entry) -> io::Result<()> {
        let mut directory = std::fs::DirBuilder::new();
        directory.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut directory, 0o700);
        directory.create(&self.dir)?;
        let text = format!(
            "{CACHE_VERSION}\n--- original\n{}--- rewrite\n{}--- evidence\n{}",
            entry.original, entry.rewrite, entry.evidence
        );
        let (mut file, temporary) = create_temporary(&self.dir, key)?;
        let written = file.write_all(text.as_bytes()).and_then(|()| file.sync_all());
        drop(file);
        let stored = written.and_then(|()| std::fs::rename(&temporary, self.dir.join(key)));
        if stored.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        stored
    }
}

/// Creates a temporary file for the entry named `key` in `dir`, readable and writable by its
/// owner alone, under a random name that nothing in `dir` has yet.
fn create_temporary(dir: &Path, key: &str) -> io::Result<(File, PathBuf)> {
    const ATTEMPTS: usize = 16;
    let mut error = None;
    for _ in 0..ATTEMPTS {
        let random = RandomState::new().build_hasher().finish();
        let path = dir.join(format!("{key}.{random:016x}.tmp"));
        let mut options = OpenOptions::new();
        // `create_new` refuses an existing path, a link included, rather than following it.
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => error = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(error.unwrap_or_else(|| io::Error::other("cannot name a temporary cache file")))
}

/// Proposes scripted candidates, ignoring verdicts.
pub(super) struct ScriptRewriter {
    candidates: FxHashMap<String, Vec<String>>,
}

impl ScriptRewriter {
    /// Reads a script.
    pub(super) fn load(path: &Path) -> Result<Self, String> {
        // Scripts are test inputs, not sources, so they bypass the source loader.
        #[allow(clippy::disallowed_methods)]
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
        Self::parse(&text).map_err(|error| format!("invalid script `{}`: {error}", path.display()))
    }

    fn parse(text: &str) -> Result<Self, String> {
        let mut candidates = FxHashMap::<String, Vec<String>>::default();
        let mut function = None::<String>;
        let preamble_end = text.find("--- ").unwrap_or(text.len());
        for (marker, text) in sections(&text[preamble_end..])? {
            if let Some(name) = marker.strip_prefix("function ") {
                if !text.trim().is_empty() {
                    return Err(format!("text between `--- function {name}` and its candidates"));
                }
                candidates.entry(name.to_string()).or_default();
                function = Some(name.to_string());
            } else if marker == "candidate" {
                let Some(function) = &function else {
                    return Err("a candidate before any `--- function`".into());
                };
                candidates.entry(function.clone()).or_default().push(text);
            } else {
                return Err(format!("unknown section `--- {marker}`"));
            }
        }
        Ok(Self { candidates })
    }
}

impl LlmRewriter for ScriptRewriter {
    fn session(&self, request: &RewriteRequest) -> Result<Box<dyn LlmSession>, LlmError> {
        let candidates = self.candidates.get(&request.function_name).cloned().unwrap_or_default();
        Ok(Box::new(ScriptSession { candidates: candidates.into_iter() }))
    }
}

struct ScriptSession {
    candidates: std::vec::IntoIter<String>,
}

impl LlmSession for ScriptSession {
    fn propose(&mut self, _verdict: Option<&Verdict>) -> Result<Proposal, LlmError> {
        Ok(self.candidates.next().map_or(Proposal::Done, Proposal::Candidate))
    }
}

/// Splits `text` at lines starting with `--- ` into the markers after them and the lines they
/// head. Text before the first marker is an error.
fn sections(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut sections = Vec::<(String, String)>::new();
    for line in text.split_inclusive('\n') {
        if let Some(marker) = line.strip_prefix("--- ") {
            sections.push((marker.trim_end().to_string(), String::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push_str(line);
        } else if !line.trim().is_empty() {
            return Err(format!("text before the first section: `{}`", line.trim_end()));
        }
    }
    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_sections() {
        let script = ScriptRewriter::parse(
            "Preamble.\n--- function f\n--- candidate\nfirst\n--- candidate\nsecond\n\
             --- function g.1\n--- candidate\nthird\n",
        )
        .unwrap();
        assert_eq!(script.candidates["f"], ["first\n", "second\n"]);
        assert_eq!(script.candidates["g.1"], ["third\n"]);
        assert!(ScriptRewriter::parse("--- candidate\nx\n").is_err());
        assert!(ScriptRewriter::parse("--- function f\nstray\n").is_err());
        assert!(ScriptRewriter::parse("--- rewrite\n").is_err());
    }

    #[test]
    fn cache_round_trip() {
        let dir = std::env::temp_dir().join(format!("solar-llm-cache-{}", std::process::id()));
        let cache = Cache::new(&dir);
        assert!(cache.load("missing.llm").unwrap().is_none());
        let entry = Entry {
            original: "fn @f() {\n}\n".into(),
            rewrite: "fn @f() {\n  bb0:\n    ret\n}\n".into(),
            evidence: "source: script\n".into(),
        };
        cache.store("entry.llm", &entry).unwrap();
        let loaded = cache.load("entry.llm").unwrap().unwrap();
        assert_eq!(
            (loaded.original, loaded.rewrite, loaded.evidence),
            (entry.original, entry.rewrite, entry.evidence)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    // The test reads back a file it wrote, not a source.
    #[allow(clippy::disallowed_methods)]
    #[cfg(unix)]
    #[test]
    fn cache_entries_are_private() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = std::env::temp_dir().join(format!("solar-llm-private-{}", std::process::id()));
        let dir = root.join("cache");
        let cache = Cache::new(&dir);
        let entry = Entry {
            original: "fn @f() {\n}\n".into(),
            rewrite: "fn @f() {\n  bb0:\n    ret\n}\n".into(),
            evidence: "source: script\n".into(),
        };
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        cache.store("entry.llm", &entry).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("entry.llm")), 0o600);

        // A link planted at an entry's name is replaced, and what it points to is untouched.
        let target = root.join("target");
        std::fs::write(&target, "kept").unwrap();
        symlink(&target, dir.join("linked.llm")).unwrap();
        cache.store("linked.llm", &entry).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "kept");
        assert!(!std::fs::symlink_metadata(dir.join("linked.llm")).unwrap().is_symlink());
        assert_eq!(mode(&dir.join("linked.llm")), 0o600);
        // No temporary file stays behind.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}
