use crate::utils::path_contains_curry;
use std::path::Path;

pub(crate) fn should_skip(path: &Path) -> Result<(), &'static str> {
    let path_contains = path_contains_curry(path);

    if path_contains("/recursion_depth.yul") {
        return Err("recursion stack overflow");
    }

    if path_contains("/verbatim") {
        return Err("verbatim Yul builtin is not implemented");
    }

    if path_contains("/period_in_identifier")
        || path_contains("/dot_middle")
        || path_contains("/leading_and_trailing_dots")
    {
        // Why does Solc parse periods as part of Yul identifiers?
        // `yul-identifier` is the same as `solidity-identifier`, which disallows periods:
        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityLexer.YulIdentifier
        return Err("not actually valid identifiers");
    }

    if path_contains("objects/conflict_") || path_contains("objects/code.yul") {
        // Not the parser's job to check conflicting names.
        return Err("not implemented in the parser");
    }

    if path_contains(".sol") {
        return Err("not a Yul file");
    }

    let stem = path.file_stem().unwrap().to_str().unwrap();
    #[rustfmt::skip]
    if matches!(
        stem,
        // Why should this fail?
        | "unicode_comment_direction_override"
        // TODO: Implement after parsing.
        | "number_literals_2"
        | "number_literals_3"
        | "number_literals_4"
        | "number_literal_2"
        | "number_literal_3"
        | "number_literal_4"
        | "pc_disallowed"
        | "for_statement_nested_continue"
        | "linkersymbol_invalid_redefine_builtin"
        // TODO: Implemented with Yul object syntax.
        | "datacopy_shadowing"
        | "dataoffset_shadowing"
        | "datasize_shadowing"
        | "linkersymbol_shadowing"
        | "loadimmutable_shadowing"
        | "setimmutable_shadowing"
        // TODO: Not in the grammar, but docs are used to denote locations in the original src.
        | "sourceLocations"
        // TODO: EVM version-aware parsing.
        | "blobbasefee_identifier_pre_cancun"
        | "blobhash_pre_cancun"
        | "mcopy_as_identifier_pre_cancun"
        | "mcopy_pre_cancun"
        | "tstore_tload_as_identifiers_pre_cancun"
    ) {
        return Err("manually skipped");
    };

    Ok(())
}
