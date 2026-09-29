use crate::test_support::TestProject;

/// Asserts the workspace root and `pkg/` remapping target that own imports from `path`.
fn assert_context(
    project: &TestProject,
    roots: &[&str],
    path: &str,
    expected: Option<(&str, &str)>,
) {
    let config = if roots.is_empty() { project.config() } else { project.config_with_roots(roots) };
    let context = config.import_resolution_context(&project.path(path)).map(|context| {
        let remapping = context
            .compile_opts()
            .import_remappings
            .iter()
            .find(|remapping| remapping.prefix == "pkg/")
            .unwrap();
        (context.workspace_root().to_path_buf(), remapping.path.clone())
    });
    assert_eq!(
        context,
        expected.map(|(root, remapping)| (project.path(root), remapping.to_string())),
        "{path}"
    );
}

#[test]
fn config_selects_the_deepest_import_resolution_context() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/outer/src/"]

        //- /src/Outer.sol
        contract Outer {}

        //- /packages/app/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/inner/src/"]

        //- /packages/app/src/Inner.sol
        contract Inner {}
        "#,
    );

    assert_context(&project, &[], "/src/Outer.sol", Some(("/", "lib/outer/src/")));
    assert_context(
        &project,
        &[],
        "/packages/app/lib/pkg/Overlay.sol",
        Some(("/packages/app", "lib/inner/src/")),
    );
}

#[test]
fn config_isolates_import_contexts_across_workspace_roots() {
    let project = TestProject::from_fixture(
        r#"
        //- /first/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/first/"]

        //- /first/src/Main.sol
        contract First {}

        //- /second/foundry.toml
        [profile.default]
        src = "../shared/contracts"
        libs = ["../dependencies/packages"]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/second/"]

        //- /second/src/Main.sol
        contract Second {}

        //- /shared/contracts/Main.sol
        contract Shared {}

        //- /dependencies/packages/pkg/Dependency.sol
        contract Dependency {}
        "#,
    );
    let roots = ["/first", "/second", "/shared", "/dependencies"];

    for (path, expected) in [
        ("/first/src/Main.sol", Some(("/first", "lib/first/"))),
        ("/second/src/Main.sol", Some(("/second", "lib/second/"))),
        // External source and import-only roots do not fall back to the first workspace.
        ("/shared/contracts/Main.sol", Some(("/second", "lib/second/"))),
        ("/dependencies/packages/pkg/Overlay.sol", Some(("/second", "lib/second/"))),
        ("/unowned/Overlay.sol", None),
    ] {
        assert_context(&project, &roots, path, expected);
    }
}

#[test]
fn config_prefers_a_deeper_external_source_root_over_an_ancestor_base_path() {
    let project = TestProject::from_fixture(
        r#"
        //- /outer/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/outer/"]

        //- /outer/packages/app/foundry.toml
        [profile.default]
        src = "../../shared"
        auto_detect_remappings = false
        remappings = ["pkg/=lib/inner/"]

        //- /outer/shared/Main.sol
        contract Shared {}
        "#,
    );

    assert_context(
        &project,
        &["/outer"],
        "/outer/shared/Main.sol",
        Some(("/outer/packages/app", "lib/inner/")),
    );
}

#[test]
fn config_owns_out_of_base_remapping_targets() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=../shared/"]

        //- /project/src/Main.sol
        import "pkg/Dependency.sol";

        //- /shared/Dependency.sol
        contract Dependency {}
        "#,
    );

    assert_context(
        &project,
        &["/project"],
        "/shared/Dependency.sol",
        Some(("/project", "../shared/")),
    );
}

#[test]
fn config_rejects_ambiguous_shared_import_contexts() {
    let project = TestProject::from_fixture(
        r#"
        //- /first/foundry.toml
        [profile.default]
        libs = ["../shared/packages"]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/first/"]

        //- /second/foundry.toml
        [profile.default]
        libs = ["../shared/packages"]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/second/"]

        //- /shared/packages/Dependency.sol
        contract Dependency {}
        "#,
    );

    assert_context(
        &project,
        &["/first", "/second", "/shared"],
        "/shared/packages/Dependency.sol",
        None,
    );
}
