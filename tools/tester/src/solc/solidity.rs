use crate::{path_contains, Runner, TestResult};
use std::{fs, path::Path};
use walkdir::WalkDir;

impl Runner {
    pub(crate) fn run_solc_solidity_tests(&self) {
        eprintln!("running Solc Solidity tests with {}", self.cmd.display());

        let (collect_time, paths) = self.time(|| {
            WalkDir::new(self.root.join("testdata/solidity/test/"))
                .sort_by_file_name()
                .into_iter()
                .map(|entry| entry.unwrap())
                .filter(|entry| entry.path().extension() == Some("sol".as_ref()))
                .collect::<Vec<_>>()
        });
        eprintln!("collected {} test files in {collect_time:#?}", paths.len());

        let run = |entry: &walkdir::DirEntry| {
            let path = entry.path();
            let rel_path = path.strip_prefix(&self.root).expect("test path not in root");

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

            if self.source_delimiter.is_match(src) || self.external_source_delimiter.is_match(src) {
                return TestResult::Skipped("matched delimiters");
            }

            let expected_error = self.get_expected_error(src);

            // TODO: Imports (don't know why it's a ParserError).
            if let Some(e) = &expected_error {
                if e.code == Some(6275) {
                    return TestResult::Skipped("imports not implemented");
                }
            }

            let mut cmd = self.cmd();
            cmd.arg(rel_path).arg("-I").arg(rel_path.parent().unwrap());
            self.run_cmd(&mut cmd, |output| match (expected_error, output.status.success()) {
                (None, true) => TestResult::Passed,
                (None, false) => {
                    eprintln!("\n---- unexpected error in {} ----", rel_path.display());
                    TestResult::Failed
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
        };
        self.run_tests(&paths, run);
    }
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

    if path_contains(path, "/_relative_imports/") || path_contains(path, "/_external/") {
        return Some("not supposed to run alone");
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
        // TODO: EVM version-aware parsing. Should this even be implemented?
        | "basefee_berlin_function"
        | "prevrandao_allowed_function_pre_paris"
        // Arbitrary `pragma experimental` values are allowed by Solc apparently.
        | "experimental_test_warning"
        
    ) {
        return Some("manually skipped");
    };

    None
}
