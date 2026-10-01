use crate::test_support::TestProject;

#[test]
fn config_selects_import_resolution_contexts() {
    for (fixture, roots, cases) in [
        // The deepest context owns imports.
        (
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
            &["/"][..],
            &[
                ("/src/Outer.sol", Some(("/", "lib/outer/src/"))),
                ("/packages/app/lib/pkg/Overlay.sol", Some(("/packages/app", "lib/inner/src/"))),
            ][..],
        ),
        // Import contexts stay isolated across workspace roots.
        (
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
            &["/first", "/second", "/shared", "/dependencies"],
            &[
                ("/first/src/Main.sol", Some(("/first", "lib/first/"))),
                ("/second/src/Main.sol", Some(("/second", "lib/second/"))),
                // External source and import-only roots do not fall back to the first workspace.
                ("/shared/contracts/Main.sol", Some(("/second", "lib/second/"))),
                ("/dependencies/packages/pkg/Overlay.sol", Some(("/second", "lib/second/"))),
                ("/unowned/Overlay.sol", None),
            ],
        ),
        // A deeper external source root wins over an ancestor base path.
        (
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
            &["/outer"],
            &[("/outer/shared/Main.sol", Some(("/outer/packages/app", "lib/inner/")))],
        ),
        // A context owns remapping targets outside its base path.
        (
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
            &["/project"],
            &[("/shared/Dependency.sol", Some(("/project", "../shared/")))],
        ),
        // Shared import-only roots with several owners are ambiguous.
        (
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
            &["/first", "/second", "/shared"],
            &[("/shared/packages/Dependency.sol", None)],
        ),
    ] {
        let project = TestProject::from_fixture(fixture);
        let config = project.config_with_roots(roots);
        // Compare the workspace root and `pkg/` remapping target that own imports from `path`.
        for &(path, expected) in cases {
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
    }
}
