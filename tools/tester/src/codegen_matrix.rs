use regex::bytes::Regex;
use ui_test::{
    CommentParser, Config, Revisioned,
    spanned::{Span, Spanned},
};

pub(crate) const NAME: &str = "codegen-matrix";

const STANDARD_REVISIONS: &[&str] = &["none", "gas", "size", "mir"];

pub(crate) fn parse(parser: &mut CommentParser<&mut Revisioned>, args: Spanned<&str>, _span: Span) {
    let mut revisions = args.split_whitespace();
    parser.check(
        args.span(),
        revisions.next() == Some("standard"),
        "`codegen-matrix` must start with `standard`",
    );
    for revision in revisions {
        parser.check(
            args.span(),
            revision.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "invalid extra `codegen-matrix` revision",
        );
    }
}

pub(crate) fn is_standard(src: &str) -> bool {
    directive(src).is_some_and(|args| args.split_whitespace().next() == Some("standard"))
}

fn directive(src: &str) -> Option<&str> {
    src.lines().find_map(|line| {
        let directive = line.trim_start().strip_prefix("//@")?;
        directive.trim().strip_prefix("codegen-matrix:").map(str::trim)
    })
}

pub(crate) fn apply(config: &mut Config, src: &str) -> bool {
    if !is_standard(src) {
        return false;
    }

    let extra_revisions = config.comment_defaults.revisions.take().unwrap_or_default();
    let mut revisions = revisions(src).into_iter().map(str::to_owned).collect::<Vec<_>>();
    for revision in extra_revisions {
        if !revisions.contains(&revision) {
            revisions.push(revision);
        }
    }
    config.comment_defaults.revisions = Some(revisions);
    let artifact_stdout = Regex::new(r"(?s).+").unwrap();
    for (revision, flags) in [
        ("none", &["-O", "none", "--emit=abi,bin"] as &[&str]),
        ("gas", &["-O", "gas", "--emit=abi,bin"]),
        ("size", &["-O", "size", "--emit=abi,bin"]),
        ("mir", &["-O", "none", "-Zdump=mir"]),
    ] {
        let defaults =
            config.comment_defaults.revisioned.entry(vec![revision.to_owned()]).or_default();
        defaults.compile_flags.extend(flags.iter().map(|flag| (*flag).to_owned()));
        if revision != "mir" {
            defaults.normalize_stdout.push((artifact_stdout.clone().into(), vec![]));
        }
    }
    true
}

pub(crate) fn revisions(src: &str) -> Vec<&str> {
    let mut revisions = STANDARD_REVISIONS.to_vec();
    for revision in directive(src).into_iter().flat_map(|args| args.split_whitespace().skip(1)) {
        if !revisions.contains(&revision) {
            revisions.push(revision);
        }
    }
    revisions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_standard_matrix() {
        assert!(is_standard("//@ codegen-matrix: standard\n"));
        assert!(is_standard("//@ codegen-matrix: standard unlinked\n"));
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
        assert!(
            config.comment_defaults.revisioned[&["mir".to_owned()][..]].normalize_stdout.is_empty()
        );
    }

    #[test]
    fn retains_extra_revisions() {
        let mut config = Config::dummy();
        assert!(apply(&mut config, "//@ codegen-matrix: standard unlinked mir\n"));
        assert_eq!(
            config.comment_defaults.revisions.as_deref(),
            Some(
                &[
                    "none".to_owned(),
                    "gas".to_owned(),
                    "size".to_owned(),
                    "mir".to_owned(),
                    "unlinked".to_owned(),
                ][..]
            )
        );
        assert_eq!(
            revisions("//@ codegen-matrix: standard unlinked\n"),
            ["none", "gas", "size", "mir", "unlinked"]
        );
    }
}
