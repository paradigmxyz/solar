use super::super::analyze;
use crate::test_support::TestProject;
use snapbox::{IntoData, assert_data_eq};
use std::fmt::Write;

#[test]
fn uses_workspace_remappings_for_import_resolution() {
    check_workspace_analysis(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib=lib/"]

        //- /src/A.sol
        import "@lib/B.sol"; contract A is B {}

        //- /lib/B.sol
        contract B {}
        "#,
        snapbox::str![[r#"
batches: 1
diagnostics: -
"#]],
    );
}

#[test]
fn resolves_relative_imports_when_cwd_differs_from_workspace_root() {
    check_workspace_analysis(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/A.sol
        import "./B.sol"; contract A is B {}

        //- /src/B.sol
        contract B {}
        "#,
        snapbox::str![[r#"
batches: 1
diagnostics: -
"#]],
    );
}

#[test]
fn uses_foundry_auto_remappings_for_import_resolution() {
    check_workspace_analysis(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/A.sol
        import "forge-std/Test.sol"; contract A is Test {}

        //- /lib/forge-std/src/Test.sol
        contract Test {}
        "#,
        snapbox::str![[r#"
batches: 1
diagnostics: -
"#]],
    );
}

fn check_workspace_analysis(fixture: &str, expected: impl IntoData) {
    let project = TestProject::from_fixture(fixture);
    let snapshot = super::snapshot(&project);

    let mut batches = snapshot.analysis_batches(Vec::new());
    let batch_count = batches.len();
    let result = analyze(batches.pop().expect("expected one analysis batch"));

    assert_data_eq!(format_analysis_result(&project, batch_count, result), expected);
}

fn format_analysis_result(
    project: &TestProject,
    batch_count: usize,
    result: super::super::AnalysisResult,
) -> String {
    let mut output = String::new();
    writeln!(&mut output, "batches: {batch_count}").unwrap();
    if result.diagnostics.is_empty() {
        output.push_str("diagnostics: -");
        return output;
    }

    output.push_str("diagnostics:\n");
    let mut diagnostics = result.diagnostics.iter().collect::<Vec<_>>();
    diagnostics.sort_by(|(left, _), (right, _)| left.as_str().cmp(right.as_str()));
    for (uri, diagnostics) in diagnostics {
        writeln!(&mut output, "{}", super::symbols::format_uri(project, uri)).unwrap();
        for diagnostic in diagnostics {
            writeln!(
                &mut output,
                "  {}:{}-{}:{} | {}",
                diagnostic.range.start.line,
                diagnostic.range.start.character,
                diagnostic.range.end.line,
                diagnostic.range.end.character,
                diagnostic.message,
            )
            .unwrap();
        }
    }
    finish_output(output)
}

fn finish_output(mut output: String) -> String {
    if output.ends_with('\n') {
        output.pop();
    }
    output
}
