//! Translation validation at MIR and EVM IR pass boundaries.
//!
//! Compare the serialized modules even for passes that do not report all their edits. A fresh
//! directory per changed pair isolates concurrent jobs and repeated passes. Successful checks
//! remove their snapshots. Other outcomes retain both inputs and the checker response for replay.

use solar_interface::Result;
use solar_sema::Gcx;
use std::{env, fmt::Display, fs, io, path::Path, process::Command};

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
        let context = format!("solar-tv: {layer} pass `{pass}` on `{module}`");
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
                Command::new(env::var_os("SOLAR_TV").unwrap_or_else(|| "solar-tv".into()))
                    .arg(layer)
                    .arg(before_path)
                    .arg(after_path)
                    .arg("--solar")
                    .arg(solar)
                    .output()
            });
        let (outcome, details) = match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if output.status.success() && stdout.trim() == "verified" {
                    eprintln!("{context}: verified");
                    return Ok(());
                }
                let outcome = match output.status.code() {
                    Some(1) if stdout.lines().next() == Some("counterexample") => "counterexample",
                    Some(2) => "inconclusive",
                    _ => "checker failed",
                };
                (
                    outcome,
                    format!(
                        "status: {}\n{stdout}{}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    ),
                )
            }
            Err(error) => ("checker failed", format!("could not run solar-tv: {error}")),
        };
        let path = pair.keep();
        let message = format!("{context}: {outcome}");
        fs::write(path.join("result.txt"), format!("{message}\n{details}\n")).map_err(io_error)?;
        let saved = format!("pair and checker output saved in `{}`", path.display());
        if outcome == "inconclusive" {
            gcx.dcx().warn(message).note(saved).emit();
            Ok(())
        } else {
            Err(gcx.dcx().err(message).note(details.trim().to_owned()).note(saved).emit())
        }
    }
}
