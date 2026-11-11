//! Translation validation at MIR and EVM IR pass boundaries.
//!
//! Compare the serialized modules even for passes that do not report all their edits. A fresh
//! directory per changed pair isolates concurrent jobs and repeated passes. Successful checks
//! remove their snapshots. Other outcomes retain both inputs and the checker response for replay.

use solar_interface::Result;
use solar_sema::Gcx;
use std::{
    env,
    fmt::Display,
    fs,
    io::{self, Write},
    path::Path,
    process::Command,
};

pub(crate) struct Snapshot<'a> {
    directory: &'a Path,
    before: String,
}

impl<'a> Snapshot<'a> {
    pub(crate) fn capture(gcx: Gcx<'a>, cached: bool, text: impl Display) -> Option<Self> {
        let directory = gcx.sess.opts.unstable.solar_tv.as_deref()?;
        (!cached).then(|| Self { directory, before: text.to_string() })
    }

    pub(crate) fn check(
        self,
        gcx: Gcx<'_>,
        layer: &str,
        pass: &str,
        module: impl Display,
        text: impl Display,
    ) -> Result {
        let after = text.to_string();
        if self.before == after {
            return Ok(());
        }
        let gas_policy = (layer == "evmir").then(|| env::var_os("SOLAR_TV_GAS")).flatten();
        let mut context = format!("solar-tv: {layer} pass `{pass}` on `{module}`");
        if let Some(policy) = &gas_policy {
            context.push_str(&format!(
                " [gas={}, evm={}]",
                policy.to_string_lossy(),
                gcx.sess.opts.evm_version
            ));
        }
        let io_error = |error: io::Error| {
            gcx.dcx()
                .err(format!(
                    "{context}: could not save audit in `{}`: {error}",
                    self.directory.display()
                ))
                .emit()
        };
        fs::create_dir_all(self.directory).map_err(io_error)?;
        let pair = tempfile::Builder::new()
            .prefix(&format!("{layer}-"))
            .tempdir_in(std::path::absolute(self.directory).map_err(io_error)?)
            .map_err(io_error)?;
        let before_path = pair.path().join(format!("before.{layer}"));
        let after_path = pair.path().join(format!("after.{layer}"));
        fs::write(&before_path, self.before).map_err(io_error)?;
        fs::write(&after_path, after).map_err(io_error)?;

        // Reparse using this compiler. Embedders can supply the compiler binary through SOLAR.
        // The validator runs the child's "none" pipeline without forwarding the audit flag.
        let output = env::var_os("SOLAR")
            .map(Ok)
            .unwrap_or_else(|| env::current_exe().map(Into::into))
            .and_then(|solar| {
                let mut command =
                    Command::new(env::var_os("SOLAR_TV").unwrap_or_else(|| "solar-tv".into()));
                command.arg(layer).arg(before_path).arg(after_path).arg("--solar").arg(solar);
                command.arg("--report").arg(pair.path().join("result.json"));
                command.arg("--evm-version").arg(gcx.sess.opts.evm_version.to_string());
                if gas_policy.is_none() {
                    // MIR has no physical stack schedule, so keep its gas-independent audit.
                    command.env_remove("SOLAR_TV_GAS");
                }
                command.output()
            });
        let mut record = None;
        let (outcome, details, diagnostic) = match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let parsed = fs::read(pair.path().join("result.json"))
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| {
                        serde_json::from_slice::<serde_json::Value>(&bytes)
                            .map_err(|error| error.to_string())
                    })
                    .and_then(|value| {
                        if value.is_object() {
                            Ok(value)
                        } else {
                            Err("audit report must be an object".into())
                        }
                    });
                let outcome = match (&parsed, output.status.code()) {
                    (Ok(value), code) if value["version"] == 1 => {
                        match (value["status"].as_str(), code) {
                            (Some("verified"), Some(0)) if stdout.trim() == "verified" => {
                                "verified"
                            }
                            (Some("counterexample"), Some(1))
                                if stdout.lines().next() == Some("counterexample") =>
                            {
                                "counterexample"
                            }
                            (Some("unsupported"), Some(2)) => "unsupported",
                            (Some("timeout"), Some(2)) => "timeout",
                            (Some("inconclusive"), Some(2)) => "inconclusive",
                            _ => "checker failed",
                        }
                    }
                    _ => "checker failed",
                };
                let mut details = format!("status: {}\n{stdout}{stderr}", output.status);
                match parsed {
                    Ok(value) => record = Some(value),
                    Err(error) => {
                        details.push_str(&format!("\nmissing or invalid audit report: {error}\n"))
                    }
                }
                let diagnostic = match outcome {
                    "counterexample" => {
                        stdout.strip_prefix("counterexample\n").unwrap_or(&stdout).trim().to_owned()
                    }
                    "unsupported" | "timeout" | "inconclusive" => {
                        stderr.strip_prefix("inconclusive: ").unwrap_or(&stderr).trim().to_owned()
                    }
                    _ => details.clone(),
                };
                (outcome, details, diagnostic)
            }
            Err(error) => {
                let details = format!("could not run solar-tv: {error}");
                ("checker failed", details.clone(), details)
            }
        };
        let mut record = record.unwrap_or_else(|| serde_json::json!({"version": 1}));
        record["context"] = serde_json::Value::String(context.clone());
        record["status"] = serde_json::Value::String(
            if outcome == "checker failed" { "error" } else { outcome }.into(),
        );
        if outcome == "verified" {
            archive_record(self.directory, &record).map_err(io_error)?;
            eprintln!("{context}: verified");
            return Ok(());
        }
        let path = pair.keep();
        let message = format!("{context}: {outcome}");
        fs::write(path.join("result.txt"), format!("{message}\n{details}\n")).map_err(io_error)?;
        record["pair"] = serde_json::Value::String(path.display().to_string());
        let json =
            serde_json::to_vec_pretty(&record).map_err(io::Error::other).map_err(io_error)?;
        fs::write(path.join("result.json"), json).map_err(io_error)?;
        archive_record(self.directory, &record).map_err(io_error)?;
        let saved = format!("pair and checker output saved in `{}`", path.display());
        if matches!(outcome, "unsupported" | "timeout" | "inconclusive") {
            gcx.dcx().warn(message).note(diagnostic).note(saved).emit();
            Ok(())
        } else {
            Err(gcx.dcx().err(message).note(diagnostic).note(saved).emit())
        }
    }
}

fn archive_record(directory: &Path, record: &serde_json::Value) -> io::Result<()> {
    let directory = directory.join("audits");
    fs::create_dir_all(&directory)?;
    let mut file =
        tempfile::Builder::new().prefix("check-").suffix(".tmp").tempfile_in(directory)?;
    serde_json::to_writer_pretty(&mut file, record).map_err(io::Error::other)?;
    file.write_all(b"\n")?;
    let path = file.path().with_extension("json");
    file.persist_noclobber(path).map_err(|error| error.error)?;
    Ok(())
}
