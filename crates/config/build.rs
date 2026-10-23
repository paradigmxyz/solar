use std::{env, io::Read, path::Path};

const SOLC_VERSION_FALLBACK: &str = "0.8.38";
#[cfg(feature = "version")]
const VERSION_UNKNOWN: &str = "unknown";

fn main() {
    let solc_version = solc_version();
    println!("cargo:rustc-env=SOLC_SEMVER_VERSION={solc_version}");

    #[cfg(feature = "version")]
    emit_version(&solc_version);
}

#[cfg(feature = "version")]
fn emit_version(solc_version: &str) {
    let info = build_info::BuildInfo::collect();
    info.emit();

    let sha = match info.sha.as_str() {
        build_info::IDEMPOTENT_OUTPUT => VERSION_UNKNOWN,
        sha => sha,
    };
    let sha_short = sha.get(..7).unwrap_or(sha);

    let is_dev = {
        let is_dirty = info.dirty == "true";

        // > git describe --always --tags
        // if not on a tag: v0.2.0-beta.3-82-g1939939b
        // if on a tag: v0.2.0-beta.3
        let not_on_tag = info.describe.ends_with(&format!("-g{sha_short}"));

        is_dirty || not_on_tag
    };
    let version_suffix = if is_dev { "-dev" } else { "" };

    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let version_suffixed = format!("{version}{version_suffix}");

    let timestamp = &info.timestamp;

    let short_version = format!("{version_suffixed} ({sha_short} {timestamp})");
    println!("cargo:rustc-env=SHORT_VERSION={short_version}");

    // Use the out dir to determine the profile being used
    let out_dir = env::var("OUT_DIR").unwrap();
    let profile = out_dir.rsplit(std::path::MAIN_SEPARATOR).nth(3).unwrap();

    let mut cargo_features = info.features.clone();
    let ignore = ["clap", "version", "serde"];
    for feature in ignore {
        cargo_features = cargo_features
            .replace(&format!(",{feature}"), "")
            .replace(&format!("{feature},"), "")
            .replace(feature, "");
    }
    let long_version = format!(
        "Version: {version}\n\
         Commit SHA: {sha}\n\
         Build Timestamp: {timestamp}\n\
         Build Features: {cargo_features}\n\
         Build Profile: {profile}",
    );
    assert_eq!(long_version.lines().count(), 5); // `version.rs` must be updated as well.
    for (i, line) in long_version.lines().enumerate() {
        println!("cargo:rustc-env=LONG_VERSION{i}={line}");
    }

    let solc_compat_version = format!("{solc_version}+commit.{sha_short}.solar.{version}");

    let solc_long_version = format!(
        "the Solidity compiler\n\
         Version: {solc_compat_version}",
    );
    assert_eq!(solc_long_version.lines().count(), 2); // `version.rs` must be updated as well.
    for (i, line) in solc_long_version.lines().enumerate() {
        println!("cargo:rustc-env=SOLC_LONG_VERSION{i}={line}");
    }
}

fn solc_version() -> String {
    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        return SOLC_VERSION_FALLBACK.to_string();
    };
    let solc_dir = Path::new(&manifest_dir).join("../../testdata/solidity");

    // A missing path would rerun the build script on every build.
    let cmake = solc_dir.join("CMakeLists.txt");
    let watched = if cmake.exists() { cmake.as_path() } else { Path::new("build.rs") };
    println!("cargo:rerun-if-changed={}", watched.display());

    solc_version_from_cmake(&solc_dir).unwrap_or_else(|_| SOLC_VERSION_FALLBACK.to_string())
}

fn solc_version_from_cmake(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmake = String::new();
    std::fs::File::open(dir.join("CMakeLists.txt"))?.read_to_string(&mut cmake)?;
    for line in cmake.lines() {
        let line = line.trim();
        if let Some(version) = line.strip_prefix("set(PROJECT_VERSION \"")
            && let Some(version) = version.strip_suffix("\")")
        {
            return Ok(version.to_string());
        }
    }
    Err("failed to determine solc version from CMakeLists.txt".into())
}

/// Build metadata, emitted as `VERGEN_*` environment variables and computed with `std` only.
///
/// The keys, values, and `rerun-if-*` lines follow `vergen-gitcl` 10, and so do the fallbacks when
/// `git` is unavailable.
#[cfg(feature = "version")]
mod build_info {
    use std::{
        env,
        fs::File,
        io::Read,
        path::{Path, PathBuf},
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    /// Value of a key that could not be computed, or that `VERGEN_IDEMPOTENT` pins.
    pub(crate) const IDEMPOTENT_OUTPUT: &str = "VERGEN_IDEMPOTENT_OUTPUT";

    const BUILD_TIMESTAMP: &str = "VERGEN_BUILD_TIMESTAMP";
    const CARGO_FEATURES: &str = "VERGEN_CARGO_FEATURES";
    const CARGO_TARGET_TRIPLE: &str = "VERGEN_CARGO_TARGET_TRIPLE";
    const GIT_DESCRIBE: &str = "VERGEN_GIT_DESCRIBE";
    const GIT_SHA: &str = "VERGEN_GIT_SHA";
    const GIT_DIRTY: &str = "VERGEN_GIT_DIRTY";

    /// Seconds from `0000-01-01T00:00:00Z` and to `9999-12-31T23:59:59Z`, the years ISO 8601 prints
    /// with four digits.
    const MIN_SECS: i64 = -62_167_219_200;
    const MAX_SECS: i64 = 253_402_300_799;

    pub(crate) struct BuildInfo {
        pub(crate) timestamp: String,
        pub(crate) features: String,
        target_triple: String,
        pub(crate) describe: String,
        pub(crate) sha: String,
        pub(crate) dirty: String,
        git_rerun_if_changed: Vec<String>,
    }

    impl BuildInfo {
        pub(crate) fn collect() -> Self {
            let idempotent = env::var("VERGEN_IDEMPOTENT").is_ok();
            let git = Git::collect(idempotent).unwrap_or_else(Git::fallback);
            Self {
                timestamp: build_timestamp(idempotent),
                features: env::var(CARGO_FEATURES).unwrap_or_else(|_| {
                    let features = env::vars().filter_map(|(k, _)| {
                        Some(k.strip_prefix("CARGO_FEATURE_")?.to_lowercase())
                    });
                    features.collect::<Vec<_>>().join(",")
                }),
                target_triple: env::var(CARGO_TARGET_TRIPLE)
                    .or_else(|_| env::var("TARGET"))
                    .unwrap_or_else(|_| IDEMPOTENT_OUTPUT.to_string()),
                describe: git.describe,
                sha: git.sha,
                dirty: git.dirty,
                git_rerun_if_changed: git.rerun_if_changed,
            }
        }

        pub(crate) fn emit(&self) {
            let vars = [
                (BUILD_TIMESTAMP, &self.timestamp),
                (CARGO_FEATURES, &self.features),
                (CARGO_TARGET_TRIPLE, &self.target_triple),
                (GIT_DESCRIBE, &self.describe),
                (GIT_SHA, &self.sha),
                (GIT_DIRTY, &self.dirty),
            ];
            for (key, value) in vars {
                println!("cargo:rustc-env={key}={}", value.replace('\n', ""));
            }
            for path in &self.git_rerun_if_changed {
                println!("cargo:rerun-if-changed={}", path.replace('\n', ""));
            }
            println!("cargo:rerun-if-changed=build.rs");
            println!("cargo:rerun-if-env-changed=VERGEN_IDEMPOTENT");
            println!("cargo:rerun-if-env-changed=VERGEN_DEFAULT_ON_ERROR");
            println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
        }
    }

    struct Git {
        describe: String,
        sha: String,
        dirty: String,
        rerun_if_changed: Vec<String>,
    }

    impl Git {
        /// Reads the metadata from the repository containing the current directory.
        ///
        /// Returns `None` if `git` is missing, this is not a worktree, or any command fails.
        /// A preset `VERGEN_GIT_*` variable replaces the value of its key.
        fn collect(idempotent: bool) -> Option<Self> {
            git(&["--version"])?;
            git(&["rev-parse", "--is-inside-work-tree"])?.contains("true").then_some(())?;

            let rerun_if_changed = if idempotent { Vec::new() } else { rerun_if_changed() };

            let dirty = match env::var(GIT_DIRTY) {
                Ok(dirty) => dirty,
                Err(_) => (!git(&["status", "--porcelain"])?.is_empty()).to_string(),
            };
            let describe = match env::var(GIT_DESCRIBE) {
                Ok(describe) => describe,
                Err(_) => {
                    let mut describe = git(&["describe", "--always"])?.trim().to_string();
                    if !git(&["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
                        describe.push_str("-dirty");
                    }
                    describe
                }
            };
            let sha = match env::var(GIT_SHA) {
                Ok(sha) => sha,
                Err(_) => git(&["rev-parse", "HEAD"])?.trim().trim_matches('\'').to_string(),
            };
            Some(Self { describe, sha, dirty, rerun_if_changed })
        }

        /// Takes the SHA and dirty flag from `.cargo_vcs_info.json`, as in a published crate, and
        /// uses the placeholder for what is still missing. A preset variable comes first.
        fn fallback() -> Self {
            let (vcs_sha, vcs_dirty) = vcs_info().unzip();
            Self {
                describe: preset_or(GIT_DESCRIBE, None),
                sha: preset_or(GIT_SHA, vcs_sha),
                dirty: preset_or(GIT_DIRTY, vcs_dirty.map(|dirty| dirty.to_string())),
                rerun_if_changed: Vec::new(),
            }
        }
    }

    /// Runs `git` and returns its standard output if it succeeds.
    fn git(args: &[&str]) -> Option<String> {
        // https://git-scm.com/docs/git-status#_background_refresh
        let output = Command::new("git").args(args).env("GIT_OPTIONAL_LOCKS", "0").output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// The `HEAD` file and the file of the branch it points to, if they exist in the git
    /// directory. In a linked worktree the branch lives in the common directory, so it is skipped.
    fn rerun_if_changed() -> Vec<String> {
        let mut paths = Vec::new();
        let Some(git_dir) = git(&["rev-parse", "--git-dir"]) else { return paths };
        let git_dir = PathBuf::from(git_dir.trim());

        let head = git_dir.join("HEAD");
        if head.exists() {
            paths.push(head.display().to_string());
        }
        if let Some(branch) = git(&["symbolic-ref", "HEAD"]) {
            let branch = git_dir.join(branch.trim());
            if branch.exists() {
                paths.push(branch.display().to_string());
            }
        }
        paths
    }

    /// The SHA and dirty flag recorded by `cargo package`.
    fn vcs_info() -> Option<(String, bool)> {
        let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")?;
        let mut json = String::new();
        File::open(Path::new(&manifest_dir).join(".cargo_vcs_info.json"))
            .ok()?
            .read_to_string(&mut json)
            .ok()?;
        let git = &json[json.find("\"git\"")?..];
        let git = &git[..git.find('}')?];
        let sha = json_value(git, "sha1")?.strip_prefix('"')?.split_once('"')?.0;
        let dirty = json_value(git, "dirty").is_some_and(|dirty| dirty.starts_with("true"));
        Some((sha.to_string(), dirty))
    }

    /// The text following `"key":` in a flat JSON object.
    fn json_value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
        let rest = &json[json.find(&format!("\"{key}\""))? + key.len() + 2..];
        Some(rest.trim_start().strip_prefix(':')?.trim_start())
    }

    /// The value of a preset variable, else `fallback`, else the placeholder for a key that could
    /// not be computed.
    fn preset_or(key: &str, fallback: Option<String>) -> String {
        env::var(key).ok().or(fallback).unwrap_or_else(|| IDEMPOTENT_OUTPUT.to_string())
    }

    /// The build time in ISO 8601, in UTC with nanoseconds, like `2023-11-14T22:13:20.000000000Z`.
    ///
    /// `SOURCE_DATE_EPOCH` replaces the clock, even with `VERGEN_IDEMPOTENT`, which otherwise
    /// pins the value to a placeholder.
    fn build_timestamp(idempotent: bool) -> String {
        let source_date_epoch = env::var_os("SOURCE_DATE_EPOCH");
        let time = match &source_date_epoch {
            Some(epoch) => epoch
                .to_str()
                .and_then(|epoch| epoch.parse::<i64>().ok())
                .filter(|secs| (MIN_SECS..=MAX_SECS).contains(secs))
                .map(|secs| (secs, 0)),
            None => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
                Some((now.as_secs() as i64, now.subsec_nanos()))
            }
        };
        let Some((secs, nanos)) = time else { return preset_or(BUILD_TIMESTAMP, None) };

        if let Ok(timestamp) = env::var(BUILD_TIMESTAMP) {
            timestamp
        } else if idempotent && source_date_epoch.is_none() {
            IDEMPOTENT_OUTPUT.to_string()
        } else {
            let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
            let secs = secs.rem_euclid(86_400);
            let (hour, minute, second) = (secs / 3600, secs % 3600 / 60, secs % 60);
            format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{nanos:09}Z")
        }
    }

    /// Converts days since 1970-01-01 to a proleptic Gregorian `(year, month, day)`.
    ///
    /// <https://howardhinnant.github.io/date_algorithms.html#civil_from_days>
    fn civil_from_days(days: i64) -> (i64, i64, i64) {
        let days = days + 719_468;
        let era = days.div_euclid(146_097);
        let day_of_era = days.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_index = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_index + 2) / 5 + 1;
        let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        (year, month, day)
    }
}
