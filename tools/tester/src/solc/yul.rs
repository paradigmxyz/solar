use crate::utils::path_contains_curry;
use solar_config::EvmVersion;
use std::path::Path;
use strum::IntoEnumIterator;

pub(crate) fn evm_version(src: &str) -> Option<EvmVersion> {
    let setting = src.lines().find_map(|line| line.trim().strip_prefix("// EVMVersion:"))?.trim();
    let mut constraints = Vec::new();
    for constraint in setting.split(',') {
        let constraint = constraint.trim();
        let (op, version) = match constraint.as_bytes() {
            [b'>', b'=', ..] => (">=", &constraint[2..]),
            [b'<', b'=', ..] => ("<=", &constraint[2..]),
            [b'>', ..] => (">", &constraint[1..]),
            [b'<', ..] => ("<", &constraint[1..]),
            [b'=', ..] => ("=", &constraint[1..]),
            _ => return None,
        };
        let version =
            if version == "current" { EvmVersion::default() } else { version.parse().ok()? };
        constraints.push((op, version));
    }
    let mut candidates = EvmVersion::iter().filter(|&candidate| {
        constraints.iter().all(|&(op, version)| match op {
            ">=" => candidate >= version,
            ">" => candidate > version,
            "<=" => candidate <= version,
            "<" => candidate < version,
            _ => candidate == version,
        })
    });
    // Pick the oldest version unless the setting only has upper bounds.
    if constraints.iter().any(|&(op, _)| matches!(op, ">=" | ">" | "=")) {
        candidates.next()
    } else {
        candidates.next_back()
    }
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
