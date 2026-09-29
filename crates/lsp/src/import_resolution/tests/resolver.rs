use super::super::{
    ImportCompletion, ImportResolutionContext, ImportResolver, MAX_IMPORT_CANDIDATES,
};
use crate::{
    test_support::TestProject,
    workspace::{Workspace, WorkspacePathIndex},
};
use std::path::PathBuf;

fn with_resolver<T>(
    project: &TestProject,
    overlay: &[PathBuf],
    f: impl FnOnce(&ImportResolver<'_, '_>, &PathBuf) -> T,
) -> T {
    let config = project.config();
    let importer = project.path("/src/Main.sol");
    let context = config.import_resolution_context(&importer).unwrap();
    f(&ImportResolver::new(context, overlay), &importer)
}

fn candidates(completion: &ImportCompletion) -> Vec<&str> {
    completion.candidates().iter().map(|candidate| candidate.import_path()).collect()
}

#[test]
fn resolver_completes_one_relative_directory_level_and_resolves_overlay_paths() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml

        //- /src/Main.sol
        import "./";

        //- /src/Local.sol
        contract Local {}

        //- /src/nested/OnDisk.sol
        contract OnDisk {}

        //- /src/README.md
        not Solidity
        "#,
    );
    let overlay =
        [project.path("/src/Unsaved.sol"), project.path("/src/virtual/OnlyInOverlay.sol")];

    with_resolver(&project, &overlay, |resolver, importer| {
        let completion = resolver.complete(importer, "./");
        assert_eq!(
            candidates(&completion),
            ["./Local.sol", "./Main.sol", "./Unsaved.sol", "./nested/", "./virtual/"]
        );
        assert!(!completion.is_incomplete());
        assert_eq!(resolver.resolve(importer, "./Unsaved.sol"), Some(overlay[0].clone()));
    });
}

#[test]
fn resolver_completes_bare_relative_directory_segments_beside_dotfiles() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml

        //- /Root.sol
        contract Root {}

        //- /src/Main.sol
        import ".";

        //- /src/Local.sol
        contract Local {}

        //- /src/.b.sol
        contract SingleDot {}

        //- /src/..b.sol
        contract DoubleDot {}
        "#,
    );

    with_resolver(&project, &[], |resolver, importer| {
        for (prefix, expected) in [
            (".", &["..b.sol", "./", ".b.sol"][..]),
            ("..", &["../", "..b.sol"]),
            ("./.", &["./..b.sol", "././", "./.b.sol"]),
            ("../..", &["../../"]),
        ] {
            let completion = resolver.complete(importer, prefix);
            assert_eq!(candidates(&completion), expected, "prefix: {prefix:?}");
        }
    });
}

#[test]
fn resolver_rejects_ambiguous_exact_imports() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        libs = ["lib", "vendor"]

        //- /src/Main.sol
        import "pkg/Target.sol";

        //- /lib/pkg/Target.sol
        contract LibTarget {}

        //- /vendor/pkg/Target.sol
        contract VendorTarget {}
        "#,
    );

    with_resolver(&project, &[], |resolver, importer| {
        assert_eq!(resolver.resolve(importer, "pkg/Target.sol"), None);
    });
}

#[test]
fn resolver_normalizes_the_foundry_root_before_applying_context_remappings() {
    let project = TestProject::from_fixture(
        r#"
        //- /container/.keep

        //- /project/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["src:pkg/=lib/"]

        //- /project/src/Main.sol
        import "pkg/Target.sol";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let manifest = project.path("/container/../project/foundry.toml");
    let workspaces = [Workspace::load_foundry(manifest).unwrap()];
    let importer = project.path("/project/src/Main.sol");
    let entries = WorkspacePathIndex::new(&workspaces).clone_import_entries();
    let context =
        ImportResolutionContext::for_workspaces_with_index(&workspaces, &importer, entries)
            .unwrap();

    let resolved = ImportResolver::new(context, &[]).resolve(&importer, "pkg/Target.sol");

    assert_eq!(resolved, Some(project.path("/project/lib/Target.sol")));
}

#[test]
fn resolver_caps_candidates_and_marks_the_result_incomplete() {
    let project = TestProject::new();
    project.write_file("/foundry.toml", "");
    project.write_file("/src/Main.sol", "import \"./\";");
    for index in 0..MAX_IMPORT_CANDIDATES {
        project.write_file(&format!("/src/Candidate{index:03}.sol"), "");
    }

    with_resolver(&project, &[], |resolver, importer| {
        let completion = resolver.complete(importer, "./");
        let candidates = candidates(&completion);
        assert_eq!(candidates.len(), MAX_IMPORT_CANDIDATES);
        assert_eq!(candidates[0], "./Candidate000.sol");
        assert_eq!(candidates[MAX_IMPORT_CANDIDATES - 1], "./Candidate255.sol");
        assert!(completion.is_incomplete());
    });
}
