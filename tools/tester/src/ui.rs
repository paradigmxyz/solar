use crate::Runner;

impl Runner {
    #[allow(unreachable_code)]
    pub(crate) fn run_ui_tests(&self) {
        // TODO
        return;

        eprintln!("running UI tests with {}", self.cmd.display());
        let path = self.root.join("tests/ui/");
        let paths = self.collect_files(&path, true);
        self.run_tests(&paths, |entry| {
            let _ = entry;
            let mut cmd = self.cmd();
            cmd.arg("--error-format=json");
            todo!();
        });
    }
}
