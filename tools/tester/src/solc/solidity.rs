use crate::{utils::path_contains, Config, TestCx, TestFns, TestResult};
use assert_cmd::Command;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{fs, path::Path};
use tempfile::TempDir;

pub(crate) const FNS: TestFns = TestFns { check, run };

static SOURCE_DELIM: Lazy<Regex> = Lazy::new(|| Regex::new(r"==== Source: (.*) ====").unwrap());
static EXT_SOURCE_DELIM: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"==== ExternalSource: (.*) ====").unwrap());

fn check(config: &Config, path: &Path) -> TestResult {
    let rel_path = path.strip_prefix(config.root).expect("test path not in root");

    if let Some(reason) = solc_solidity_filter(rel_path) {
        return TestResult::Skipped(reason);
    }

    let Ok(src) = fs::read_to_string(path) else {
        return TestResult::Skipped("invalid UTF-8");
    };
    let src = src.as_str();

    if src.contains("pragma experimental solidity") {
        return TestResult::Skipped("experimental solidity");
    }

    TestResult::Passed
}

fn run(cx: &TestCx<'_>) -> TestResult {
    let path = cx.paths.file.as_path();
    let mut cmd = cx.cmd();
    let _guard = if has_delimiters(&cx.src) {
        handle_delimiters(&cx.src, path, &mut cmd)
    } else {
        cmd.arg(path).arg("-I").arg(path.parent().unwrap());
        None
    };
    let output = cx.run_cmd(cmd);
    cx.check_expected_errors(&output);
    TestResult::Passed
}

fn solc_solidity_filter(path: &Path) -> Option<&'static str> {
    if path_contains(path, "/libyul/") {
        return Some("actually a Yul test");
    }

    if path_contains(path, "/cmdlineTests/") {
        return Some("CLI tests do not have the same format as everything else");
    }

    if path_contains(path, "/lsp/") {
        return Some("LSP tests do not have the same format as everything else");
    }

    if path_contains(path, "/ASTJSON/") {
        return Some("no JSON AST");
    }

    if path_contains(path, "/experimental/") {
        return Some("solidity experimental is not implemented");
    }

    // We don't parse licenses.
    if path_contains(path, "/license/") {
        return Some("licenses are not checked");
    }

    if path_contains(path, "natspec") {
        return Some("natspec is not checked");
    }

    if path_contains(path, "_direction_override") {
        return Some("Unicode direction override checks not implemented");
    }

    if path_contains(path, "max_depth_reached_") {
        return Some("recursion guard will not be implemented");
    }

    if path_contains(path, "wrong_compiler_") {
        return Some("Solidity pragma version is not checked");
    }

    if path_contains(path, "/_")
        && !path.components().last().unwrap().as_os_str().to_str().unwrap().starts_with('_')
    {
        // Directories starting with `_` are not tests.
        return Some("supporting file");
    }

    let stem = path.file_stem().unwrap().to_str().unwrap();
    #[rustfmt::skip]
    if matches!(
        stem,
        // Exponent is too large, but apparently it's fine in Solc because the result is 0.
        | "rational_number_exp_limit_fine"
        // `address payable` is allowed by the grammar (see `elementary-type-name`), but not by Solc.
        | "address_payable_type_expression"
        | "mapping_from_address_payable"
        // `hex` is not a keyword, looks like just a Solc limitation?
        | "hex_as_identifier"
        // TODO: These should be checked after parsing.
        | "assembly_invalid_type"
        | "assembly_dialect_leading_space"
        // `1wei` gets lexed as two different tokens, I think it's fine.
        | "invalid_denomination_no_whitespace"
        // Not actually a broken version, we just don't check "^0 and ^1".
        | "broken_version_1"
        // TODO: CBA to implement.
        | "unchecked_while_body"
        // TODO: EVM version-aware parsing.
        | "basefee_berlin_function"
        | "prevrandao_allowed_function_pre_paris"
        | "blobbasefee_shanghai_function"
        | "blobhash_pre_cancun"
        | "mcopy_as_identifier_pre_cancun"
        | "tload_tstore_not_reserved_before_cancun"
        | "blobhash_pre_cancun_not_reserved"
        // Arbitrary `pragma experimental` values are allowed by Solc apparently.
        | "experimental_test_warning"
        // "." is not a valid import path.
        | "boost_filesystem_bug"
    ) {
        return Some("manually skipped");
    };

    None
}

fn has_delimiters(src: &str) -> bool {
    src.lines().any(|s| s.starts_with("==== "))
}

fn handle_delimiters(src: &str, path: &Path, cmd: &mut Command) -> Option<TempDir> {
    let mut tempdir = None;
    let insert_tempdir =
        || tempfile::Builder::new().prefix(path.file_stem().unwrap()).tempdir().unwrap();
    let mut lines = src.lines().peekable();
    let mut add_import_path = false;
    while let Some(line) = lines.next() {
        if let Some(cap) = SOURCE_DELIM.captures(line) {
            let mut name = cap.get(1).unwrap().as_str();
            if name == "////" {
                name = "test.sol";
            }

            let mut contents = String::with_capacity(src.len());
            while lines.peek().is_some_and(|l| !l.starts_with("====")) {
                contents.push_str(lines.next().unwrap());
                contents.push('\n');
            }

            let tempdir = tempdir.get_or_insert_with(insert_tempdir);
            let path = tempdir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, contents).unwrap();
            cmd.arg(path);
        } else if let Some(cap) = EXT_SOURCE_DELIM.captures(line) {
            let eq = cap.get(1).unwrap().as_str().to_owned();
            if eq.contains('=') {
                cmd.arg("-m").arg(eq);
            }
            add_import_path = true;
        } else {
            // Sometimes `==== Source: ... ====` is missing after external sources.
            let mut contents = String::with_capacity(src.len());
            for line in lines.by_ref() {
                assert!(!line.starts_with("===="));
                contents.push_str(line);
                contents.push('\n');
            }
            let tempdir = tempdir.get_or_insert_with(insert_tempdir);
            let path = tempdir.path().join("test.sol");
            fs::write(&path, contents).unwrap();
            cmd.arg(path);
        }
    }
    if let Some(tempdir) = &tempdir {
        cmd.arg("-I").arg(tempdir.path());
    }
    if add_import_path {
        cmd.arg("-I").arg(path.parent().unwrap());
    }
    tempdir
}
