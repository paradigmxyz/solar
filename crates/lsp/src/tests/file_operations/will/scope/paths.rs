use super::*;
use std::os::unix::fs::symlink;

#[test]
fn will_file_operations_reject_source_symlinks_into_dependencies() {
    for directory in [false, true] {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            auto_detect_remappings = false

            //- /src/Importer.sol open
            import "../Target.sol";

            //- /Target.sol
            contract Target {}

            //- /lib/dep/src/Importer.sol
            import "../Target.sol";
            "#,
        );
        let importer = project.path("/src/Importer.sol");
        let target = project.path("/Target.sol");
        let mut state = state(&project);
        assert!(state.config.tracks_source_file(&importer));
        assert!(!state.config.may_omit_source_files());
        let links = state.symbol_tables.load().document_links(&importer);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, Some(Url::from_file_path(&target).unwrap()));
        for edit in will_edits(&mut state, &target) {
            let changes = edit.unwrap().changes.unwrap();
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[&Url::from_file_path(&importer).unwrap()].len(), 1);
        }

        if directory {
            fs::remove_dir_all(project.path("/src")).unwrap();
            symlink(project.path("/lib/dep/src"), project.path("/src")).unwrap();
        } else {
            fs::remove_file(&importer).unwrap();
            symlink(project.path("/lib/dep/src/Importer.sol"), &importer).unwrap();
        }

        assert!(state.config.workspace_edit_scope().check(&importer).is_ok());
        for edit in will_edits(&mut state, &target) {
            assert!(edit.is_none(), "dependency aliases must reject the entire plan: {edit:?}");
        }
    }
}

#[test]
fn will_file_operations_refresh_retargeted_dependency_symlinks() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false

        //- /src/Importer.sol open
        import "../Target.sol";

        //- /Target.sol
        contract Target {}

        //- /vendor_old/Empty.sol
        contract Empty {}

        //- /vendor_new/Importer.sol
        import "../Target.sol";
        "#,
    );
    let library = project.path("/lib");
    let importer = project.path("/src/Importer.sol");
    let dependency = project.path("/vendor_new/Importer.sol");
    let target = project.path("/Target.sol");
    symlink(project.path("/vendor_old"), &library).unwrap();
    let mut state = state(&project);
    let config = Arc::clone(&state.config);
    assert!(!config.may_omit_source_files());
    for source in [&importer, &dependency] {
        assert!(config.tracks_source_file(source));
        let links = state.symbol_tables.load().document_links(source);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, Some(Url::from_file_path(&target).unwrap()));
    }
    fs::remove_file(&importer).unwrap();
    symlink(&dependency, &importer).unwrap();
    for edit in will_edits(&mut state, &target) {
        let changes = edit.unwrap().changes.unwrap();
        assert_eq!(changes.len(), 2);
        for source in [&importer, &dependency] {
            assert_eq!(changes[&Url::from_file_path(source).unwrap()].len(), 1);
        }
    }

    fs::remove_file(&library).unwrap();
    symlink(project.path("/vendor_new"), &library).unwrap();

    assert!(Arc::ptr_eq(&state.config, &config));
    for edit in will_edits(&mut state, &target) {
        assert!(edit.is_none(), "retargeted dependencies must reject the entire plan: {edit:?}");
    }
    assert!(Arc::ptr_eq(&state.config, &config));
}

#[test]
fn will_file_operations_reject_dangling_links_but_allow_unsaved_importers() {
    for link in [false, true] {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]

            //- /src/Importer.sol open
            import "../Target.sol";

            //- /Target.sol
            contract Target {}
            "#,
        );
        let importer = project.path("/src/Importer.sol");
        let target = project.path("/Target.sol");
        let mut state = state(&project);
        assert!(state.config.tracks_source_file(&importer));
        assert!(!state.config.may_omit_source_files());
        let links = state.symbol_tables.load().document_links(&importer);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, Some(Url::from_file_path(&target).unwrap()));
        fs::remove_file(&importer).unwrap();
        if link {
            symlink(project.path("/lib/dep/Missing.sol"), &importer).unwrap();
        }

        for edit in will_edits(&mut state, &target) {
            if link {
                assert!(edit.is_none(), "dangling dependency links must be rejected: {edit:?}");
            } else {
                let changes = edit.unwrap().changes.unwrap();
                assert_eq!(changes.len(), 1);
                assert_eq!(changes[&Url::from_file_path(&importer).unwrap()].len(), 1);
            }
        }
    }
}
