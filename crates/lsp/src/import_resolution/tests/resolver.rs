use super::super::{ImportResolutionContext, ImportResolver, MAX_IMPORT_CANDIDATES};
use crate::{test_support::TestProject, workspace::Workspace};

#[test]
fn resolver_completes_only_one_relative_directory_level_and_includes_overlay_paths() {
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
    let config = project.config();
    let importer = project.path("/src/Main.sol");
    let context = config.import_resolution_context(&importer).unwrap();
    let overlay =
        [project.path("/src/Unsaved.sol"), project.path("/src/virtual/OnlyInOverlay.sol")];

    let completion = ImportResolver::new(context, &overlay).complete(&importer, "./");
    let candidates =
        completion.candidates().iter().map(|candidate| candidate.import_path()).collect::<Vec<_>>();

    assert_eq!(
        candidates,
        vec!["./Local.sol", "./Main.sol", "./Unsaved.sol", "./nested/", "./virtual/"]
    );
    assert!(!completion.is_incomplete());
}

#[test]
fn resolver_completes_bare_relative_directory_segments() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml

        //- /Root.sol
        contract Root {}

        //- /src/Main.sol
        import ".";

        //- /src/Local.sol
        contract Local {}
        "#,
    );
    let config = project.config();
    let importer = project.path("/src/Main.sol");
    let context = config.import_resolution_context(&importer).unwrap();
    let resolver = ImportResolver::new(context, &[]);

    for (prefix, expected) in [(".", "./"), ("..", "../"), ("./.", "././"), ("../..", "../../")] {
        let completion = resolver.complete(&importer, prefix);
        assert_eq!(
            completion
                .candidates()
                .iter()
                .map(|candidate| candidate.import_path())
                .collect::<Vec<_>>(),
            [expected],
            "prefix: {prefix:?}"
        );
    }
}

#[test]
fn resolver_keeps_dotfiles_beside_relative_directory_continuations() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml

        //- /src/Main.sol
        import ".";

        //- /src/.b.sol
        contract SingleDot {}

        //- /src/..b.sol
        contract DoubleDot {}
        "#,
    );
    let config = project.config();
    let importer = project.path("/src/Main.sol");
    let context = config.import_resolution_context(&importer).unwrap();
    let resolver = ImportResolver::new(context, &[]);

    for (prefix, expected) in
        [(".", vec!["..b.sol", "./", ".b.sol"]), ("..", vec!["../", "..b.sol"])]
    {
        let completion = resolver.complete(&importer, prefix);
        assert_eq!(
            completion
                .candidates()
                .iter()
                .map(|candidate| candidate.import_path())
                .collect::<Vec<_>>(),
            expected,
            "prefix: {prefix:?}"
        );
    }
}

#[test]
fn resolver_resolves_an_overlay_only_import() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml

        //- /src/Main.sol
        import "./Unsaved.sol";
        "#,
    );
    let config = project.config();
    let importer = project.path("/src/Main.sol");
    let target = project.path("/src/Unsaved.sol");
    let context = config.import_resolution_context(&importer).unwrap();
    let overlay = [target.clone()];

    let resolved = ImportResolver::new(context, &overlay).resolve(&importer, "./Unsaved.sol");

    assert_eq!(resolved, Some(target));
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
    let config = project.config();
    let importer = project.path("/src/Main.sol");
    let context = config.import_resolution_context(&importer).unwrap();

    let resolved = ImportResolver::new(context, &[]).resolve(&importer, "pkg/Target.sol");

    assert_eq!(resolved, None);
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
    let context = ImportResolutionContext::for_workspaces(&workspaces, &importer).unwrap();

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
    let config = project.config();
    let importer = project.path("/src/Main.sol");
    let context = config.import_resolution_context(&importer).unwrap();

    let completion = ImportResolver::new(context, &[]).complete(&importer, "./");

    assert_eq!(completion.candidates().len(), MAX_IMPORT_CANDIDATES);
    assert_eq!(completion.candidates()[0].import_path(), "./Candidate000.sol");
    assert_eq!(
        completion.candidates()[MAX_IMPORT_CANDIDATES - 1].import_path(),
        "./Candidate255.sol"
    );
    assert!(completion.is_incomplete());
}
