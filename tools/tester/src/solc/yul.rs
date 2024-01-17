use crate::{path_contains, Runner, TestResult};
use std::{fs, path::Path};

impl Runner {
    pub(crate) fn run_solc_yul_test(&self, path: &Path, check: bool) -> TestResult {
        let rel_path = path.strip_prefix(self.root).expect("test path not in root");

        if let Some(reason) = solc_yul_filter(path) {
            return TestResult::Skipped(reason);
        }

        let Ok(src) = fs::read_to_string(path) else {
            return TestResult::Skipped("invalid UTF-8");
        };
        let src = src.as_str();

        if check {
            return TestResult::Passed;
        }

        let error = self.get_expected_error(src);

        let mut cmd = self.cmd();
        cmd.arg("--language=yul").arg(rel_path);
        self.run_cmd(&mut cmd, |output| match (error, output.status.success()) {
            (None, true) => TestResult::Passed,
            (None, false) => {
                // TODO: Typed identifiers.
                if String::from_utf8_lossy(&output.stderr).contains("found `:`") {
                    TestResult::Skipped("typed identifiers")
                } else {
                    eprintln!("\n---- unexpected error in {} ----", rel_path.display());
                    TestResult::Failed
                }
            }
            (Some(e), true) => {
                if e.kind.parse_time_error() {
                    eprintln!("\n---- unexpected success in {} ----", rel_path.display());
                    eprintln!("-- expected error --\n{e}");
                    TestResult::Failed
                } else {
                    TestResult::Passed
                }
            }
            (Some(_e), false) => TestResult::Passed,
        })
    }
}

fn solc_yul_filter(path: &Path) -> Option<&'static str> {
    if path_contains(path, "/recursion_depth.yul") {
        return Some("recursion stack overflow");
    }

    if path_contains(path, "/verbatim") {
        return Some("verbatim Yul builtin is not implemented");
    }

    if path_contains(path, "/period_in_identifier") || path_contains(path, "/dot_middle") {
        // Why does Solc parse periods as part of Yul identifiers?
        // `yul-identifier` is the same as `solidity-identifier`, which disallows periods:
        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityLexer.YulIdentifier
        return Some("not actually valid identifiers");
    }

    if path_contains(path, "objects/conflict_") || path_contains(path, "objects/code.yul") {
        // Not the parser's job to check conflicting names.
        return Some("not implemented in the parser");
    }

    let stem = path.file_stem().unwrap().to_str().unwrap();
    #[rustfmt::skip]
    if matches!(
        stem,
        // Why should this fail?
        | "unicode_comment_direction_override"
        // TODO: Probably implement outside of parsing.
        | "number_literals_3"
        | "number_literals_4"
        // TODO: Implemented with Yul object syntax.
        | "datacopy_shadowing"
        | "dataoffset_shadowing"
        | "datasize_shadowing"
        | "linkersymbol_shadowing"
        | "loadimmutable_shadowing"
        | "setimmutable_shadowing"
        // TODO: Special case this in the parser?
        | "pc_disallowed"
        // TODO: Not parser related, but should be implemented later.
        | "for_statement_nested_continue"
        // TODO: Not in the grammar, but docs are used to denote locations in the original src.
        | "sourceLocations"
    ) {
        return Some("manually skipped");
    };
    None
}
