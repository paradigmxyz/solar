use crate::test_support::TestProject;

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
    let config = project.config();

    let outer = config.import_resolution_context(&project.path("/src/Outer.sol")).unwrap();
    let inner = config
        .import_resolution_context(&project.path("/packages/app/lib/pkg/Overlay.sol"))
        .unwrap();

    assert_eq!(outer.workspace_root(), project.root());
    assert_eq!(inner.workspace_root(), project.path("/packages/app"));
    assert_eq!(
        outer
            .compile_opts()
            .import_remappings
            .iter()
            .find(|remapping| remapping.prefix == "pkg/")
            .unwrap()
            .path,
        "lib/outer/src/"
    );
    assert_eq!(
        inner
            .compile_opts()
            .import_remappings
            .iter()
            .find(|remapping| remapping.prefix == "pkg/")
            .unwrap()
            .path,
        "lib/inner/src/"
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
        auto_detect_remappings = false
        remappings = ["pkg/=lib/second/"]

        //- /second/src/Main.sol
        contract Second {}
        "#,
    );
    let config = project.config_with_roots(&["/first", "/second"]);

    let first = config.import_resolution_context(&project.path("/first/src/Main.sol")).unwrap();
    let second = config.import_resolution_context(&project.path("/second/src/Main.sol")).unwrap();

    assert_eq!(first.workspace_root(), project.path("/first"));
    assert_eq!(second.workspace_root(), project.path("/second"));
    assert_eq!(first.compile_opts().import_remappings[0].path, "lib/first/");
    assert_eq!(second.compile_opts().import_remappings[0].path, "lib/second/");
}

#[test]
fn config_owns_external_source_roots_without_falling_back_to_the_first_workspace() {
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
        auto_detect_remappings = false
        remappings = ["pkg/=lib/second/"]

        //- /shared/contracts/Main.sol
        contract Shared {}
        "#,
    );
    let config = project.config_with_roots(&["/first", "/second", "/shared"]);

    let context =
        config.import_resolution_context(&project.path("/shared/contracts/Main.sol")).unwrap();

    assert_eq!(context.workspace_root(), project.path("/second"));
    assert_eq!(context.compile_opts().import_remappings[0].path, "lib/second/");
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
    let config = project.config_with_roots(&["/outer"]);

    let context =
        config.import_resolution_context(&project.path("/outer/shared/Main.sol")).unwrap();

    assert_eq!(context.workspace_root(), project.path("/outer/packages/app"));
    assert_eq!(context.compile_opts().import_remappings[0].path, "lib/inner/");
}

#[test]
fn config_owns_external_import_only_roots_without_cross_project_contamination() {
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
        libs = ["../dependencies/packages"]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/second/"]

        //- /second/src/Main.sol
        contract Second {}

        //- /dependencies/packages/pkg/Dependency.sol
        contract Dependency {}
        "#,
    );
    let config = project.config_with_roots(&["/first", "/second", "/dependencies"]);

    let context = config
        .import_resolution_context(&project.path("/dependencies/packages/pkg/Overlay.sol"))
        .unwrap();

    assert_eq!(context.workspace_root(), project.path("/second"));
    assert_eq!(context.compile_opts().import_remappings[0].path, "lib/second/");
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
    let config = project.config_with_roots(&["/project"]);

    let context =
        config.import_resolution_context(&project.path("/shared/Dependency.sol")).unwrap();

    assert_eq!(context.workspace_root(), project.path("/project"));
    assert_eq!(context.compile_opts().import_remappings[0].path, "../shared/");
}

#[test]
fn config_does_not_guess_an_import_context_for_an_unowned_path() {
    let project = TestProject::from_fixture(
        r#"
        //- /first/foundry.toml

        //- /first/src/Main.sol
        contract First {}

        //- /second/foundry.toml

        //- /second/src/Main.sol
        contract Second {}
        "#,
    );
    let config = project.config_with_roots(&["/first", "/second"]);

    assert!(config.import_resolution_context(&project.path("/unowned/Overlay.sol")).is_none());
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
    let config = project.config_with_roots(&["/first", "/second", "/shared"]);

    assert!(
        config
            .import_resolution_context(&project.path("/shared/packages/Dependency.sol"))
            .is_none()
    );
}
