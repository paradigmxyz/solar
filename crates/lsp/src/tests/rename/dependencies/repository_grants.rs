use super::*;

#[tokio::test]
async fn discovered_repository_manifest_cannot_grant_other_projects_dependencies() {
    let fixture = RequestFixture::new(
        r#"
        //- /a/foundry.toml
        [profile.default]
        libs = ["vendor"]
        auto_detect_remappings = false
        //- /a/vendor/owned/Owned.sol
        contract $1Owned {}
        //- /a/src/Main.sol
        import "../vendor/owned/Owned.sol";
        contract Main is $2Owned {}
        //- /b/foundry.toml
        [profile.default]
        src = "../c/modules/dep/src"
        libs = []
        auto_detect_remappings = false
        //- /c/foundry.toml
        [profile.default]
        libs = []
        auto_detect_remappings = false
        //- /c/modules/dep/.git
        gitdir: elsewhere
        //- /c/modules/dep/src/foundry.toml
        [profile.default]
        src = "../../../../a/vendor/owned"
        libs = []
        auto_detect_remappings = false
        //- /c/modules/dep/src/Dep.sol
        contract Dep {}
        "#,
        "/a/src/Main.sol",
    );
    let allowed = str![[r#"
/a/src/Main.sol:1:17-1:22 -> Renamed
/a/vendor/owned/Owned.sol:0:9-0:14 -> Renamed

"#]];
    for (roots, expected) in
        [(&["/"][..], DEPENDENCY.into_data()), (&["/", "/c/modules/dep/src"], allowed.into())]
    {
        for marker in ["$1", "$2"] {
            let (mut state, params) = fixture.rename_state_with_roots(marker, "Renamed", roots);
            // A source grant discovers the repository's manifest without making that
            // manifest independent authorization to edit another project's dependencies.
            assert_eq!(state.config.workspaces().len(), 4);
            let dependency =
                workspace_at(&state.config, &fixture.project_path("/c/modules/dep/src"));
            assert!(
                dependency.import_source_roots().contains(&fixture.project_path("/a/vendor/owned"))
            );
            check_report(&fixture, &mut state, params, expected.clone()).await;
        }
    }
}
