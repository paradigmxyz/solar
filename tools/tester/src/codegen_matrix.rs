use ui_test::{
    CommentParser, Config, Revisioned,
    spanned::{Span, Spanned},
};

pub(crate) const NAME: &str = "codegen-matrix";

const STANDARD_REVISIONS: &[&str] = &["none", "gas", "size", "mir"];

pub(crate) fn parse(parser: &mut CommentParser<&mut Revisioned>, args: Spanned<&str>, _span: Span) {
    parser.check(
        args.span(),
        args.trim() == "standard",
        "`codegen-matrix` only supports `standard`",
    );
}

pub(crate) fn is_standard(src: &str) -> bool {
    src.lines().any(|line| {
        let Some(directive) = line.trim_start().strip_prefix("//@") else {
            return false;
        };
        directive.trim() == "codegen-matrix: standard"
    })
}

pub(crate) fn apply(config: &mut Config, src: &str) -> bool {
    if !is_standard(src) {
        return false;
    }

    config.comment_defaults.revisions =
        Some(STANDARD_REVISIONS.iter().map(|revision| (*revision).to_owned()).collect());
    for (revision, flags) in [
        ("none", &["-O", "none", "--emit=abi,bin"] as &[&str]),
        ("gas", &["-O", "gas", "--emit=abi,bin"]),
        ("size", &["-O", "size", "--emit=abi,bin"]),
        ("mir", &["-O", "none", "-Zdump=mir"]),
    ] {
        config
            .comment_defaults
            .revisioned
            .entry(vec![revision.to_owned()])
            .or_default()
            .compile_flags
            .extend(flags.iter().map(|flag| (*flag).to_owned()));
    }
    true
}

pub(crate) fn revisions() -> impl Iterator<Item = &'static str> {
    STANDARD_REVISIONS.iter().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_standard_matrix() {
        assert!(is_standard("//@ codegen-matrix: standard\n"));
        assert!(is_standard("  //@codegen-matrix: standard\n"));
        assert!(!is_standard("//@ codegen-matrix: custom\n"));
    }

    #[test]
    fn applies_standard_matrix() {
        let mut config = Config::dummy();
        assert!(apply(&mut config, "//@ codegen-matrix: standard\n"));
        assert_eq!(
            config.comment_defaults.revisions.as_deref(),
            Some(&["none".to_owned(), "gas".to_owned(), "size".to_owned(), "mir".to_owned()][..])
        );
        assert_eq!(
            config.comment_defaults.revisioned[&["mir".to_owned()][..]].compile_flags,
            ["-O", "none", "-Zdump=mir"].map(str::to_owned)
        );
    }
}
