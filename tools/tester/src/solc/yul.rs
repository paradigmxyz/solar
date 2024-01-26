use crate::{utils::path_contains, Config, TestCx, TestFns, TestResult};
use std::{fs, path::Path};

pub(crate) const FNS: TestFns = TestFns { check, run };

fn check(_config: &Config, path: &Path) -> TestResult {
    if let Some(reason) = solc_yul_filter(path) {
        return TestResult::Skipped(reason);
    }

    if fs::read_to_string(path).is_err() {
        return TestResult::Skipped("invalid UTF-8");
    }

    TestResult::Passed
}

fn run(cx: &TestCx<'_>) -> TestResult {
    let path = cx.paths.file.as_path();
    let mut cmd = cx.cmd();
    cmd.arg(path).arg("--language=yul").arg("-Zparse-yul");
    let output = cx.run_cmd(cmd);
    // TODO: Typed identifiers.
    if output.stderr.contains("found `:`") {
        return TestResult::Skipped("typed identifiers");
    }
    cx.check_expected_errors(&output);
    TestResult::Passed
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
        // TODO: EVM version-aware parsing.
        | "blobbasefee_identifier_pre_cancun"
        | "blobhash_pre_cancun"
        | "mcopy_as_identifier_pre_cancun"
        | "mcopy_pre_cancun"
        | "tstore_tload_as_identifiers_pre_cancun"
    ) {
        return Some("manually skipped");
    };
    None
}
