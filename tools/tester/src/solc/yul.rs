use crate::utils::path_contains;
use regex::Regex;
use std::{path::Path, sync::LazyLock};

pub(crate) fn should_skip(path: &Path) -> Option<&'static str> {
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

    if has_typed_identifier(path) {
        return Some("typed identifiers are not implemented");
    }

    None
}

fn has_typed_identifier(path: &Path) -> bool {
    static TYPED_IDENTIFIER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\w+:\s*\w+").unwrap());

    let Ok(s) = std::fs::read_to_string(path) else { return false };
    TYPED_IDENTIFIER_RE.is_match(&s)
}
