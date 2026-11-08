//! Adds sidecar debug-info auditing to every test compilation when `SOLAR_DEBUGIFY_DIR` is set.
//! Explicit debugify diagnostics stay intact, and ordinary stdout/stderr snapshots still apply.
//! Each test and revision has a directory; compiler processes write distinct JSON-line files there.

use std::{ffi::OsString, path::PathBuf, process::Command};
use ui_test::{
    Errored, build_manager::BuildManager, custom_flags::Flag, per_test_config::TestConfig,
};

#[derive(Clone, Debug)]
pub(super) struct Audit {
    pub output: PathBuf,
}

impl Audit {
    pub(super) const NAME: &'static str = "debugify-audit";
}

impl Flag for Audit {
    fn clone_inner(&self) -> Box<dyn Flag> {
        Box::new(self.clone())
    }

    fn apply(
        &self,
        cmd: &mut Command,
        config: &TestConfig,
        _: &BuildManager,
    ) -> Result<(), Errored> {
        let path = config.status.path();
        let relative = path.strip_prefix(&config.config.root_dir).unwrap();
        let revision = config.status.revision();
        let output =
            self.output.join(relative).join(if revision.is_empty() { "default" } else { revision });
        let mut flag = OsString::from("-Zdebugify-output=");
        flag.push(output);
        cmd.arg(flag);
        Ok(())
    }

    fn must_be_unique(&self) -> bool {
        true
    }
}
