use crate::utils::path_contains_curry;
use solar_config::EvmVersion;
use std::path::Path;
use strum::IntoEnumIterator;

pub(crate) fn evm_version(src: &str) -> Option<EvmVersion> {
    let version = src.lines().find_map(|line| line.trim().strip_prefix("// EVMVersion:"))?.trim();
    let (op, version) = match version.as_bytes() {
        [b'>', b'=', ..] => (">=", &version[2..]),
        [b'<', b'=', ..] => ("<=", &version[2..]),
        [b'>', ..] => (">", &version[1..]),
        [b'<', ..] => ("<", &version[1..]),
        [b'=', ..] => ("=", &version[1..]),
        _ => return None,
    };
    let version =
        if version == "current" { EvmVersion::default() } else { version.parse().ok()? };
    Some(match op {
        ">=" | "=" => version,
        ">" => EvmVersion::iter().find(|&candidate| candidate > version)?,
        "<=" => EvmVersion::iter().rev().find(|&candidate| candidate <= version)?,
        "<" => EvmVersion::iter().rev().find(|&candidate| candidate < version)?,
        _ => unreachable!(),
    })
}

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
    if path_contains("/yulSyntaxTests/") && matches!(
        stem,
        | "assignment_to_builtin"
        | "blobbasefee_reserved_identifier_post_cancun"
        | "blobhash"
        | "builtin_identifier_1"
        | "builtin_identifier_2"
        | "builtin_identifier_3"
        | "builtin_identifier_4"
        | "builtin_identifier_5"
        | "builtin_identifier_6"
        | "builtin_identifier_7"
        | "clash_with_non_reserved_pure_yul_builtin"
        | "clash_with_reserved_builtin"
        | "clash_with_reserved_pure_yul_builtin"
        | "clz"
        | "datacopy_shadowing"
        | "dataoffset_shadowing"
        | "datasize_shadowing"
        | "for_expr_invalid_5"
        | "functional_partial"
        | "if_statement_invalid_1"
        | "if_statement_invalid_4"
        | "linkersymbol_invalid_redefine_builtin"
        | "linkersymbol_shadowing"
        | "loadimmutable_shadowing"
        | "mcopy_as_identifier"
        | "opcode_for_function_args_1"
        | "opcode_for_function_args_2"
        | "opcode_for_functions"
        | "setimmutable_shadowing"
        | "slotnum_reserved_identifier_post_amsterdam"
        | "switch_invalid_expr_2"
        | "tload_as_identifier_post_cancun"
        | "tstore_as_identifier_post_cancun"
    ) {
        return Err("Yul builtin names are checked after parsing");
    };

    #[rustfmt::skip]
    if matches!(
        stem,
        // TODO: Why should this fail?
        | "unicode_comment_direction_override"
        // TODO: Implement after parsing.
        | "number_literals_2"
        | "number_literals_3"
        | "number_literals_4"
        | "number_literal_2"
        | "number_literal_3"
        | "number_literal_4"
        | "data_name_with_literal_newline"
    ) {
        return Err("manually skipped");
    };

    Ok(())
}
