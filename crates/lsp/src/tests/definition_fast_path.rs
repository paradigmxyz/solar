use super::support::RequestFixture;
use crate::vfs::VfsPath;
use crop::Rope;
use lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, PartialResultParams, Position,
    TextDocumentIdentifier, TextDocumentPositionParams, Url, WorkDoneProgressParams,
};
use snapbox::str;

#[test]
fn current_open_code_and_import_literals_keep_distinct_targets() {
    let fixture = RequestFixture::new(
        r#"
        //- /Main.sol open
        import "./$1Target.sol";
        contract Main {
            function target() internal {}
            function caller() external { $2target(); }
        }

        //- /Target.sol
        contract Target {}
        "#,
        "/Main.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/Target.sol:0:0 contract Target {}

"#]],
    );
    fixture.check_goto_definition(
        "$2",
        str![[r#"
/Main.sol:2:13 function target() internal {}

"#]],
    );
}

#[tokio::test(flavor = "current_thread")]
async fn changed_open_import_does_not_use_an_old_code_symbol() {
    let fixture = RequestFixture::new(
        r#"
        //- /Main.sol open
        contract $1Main {}

        //- /Target.sol
        contract Target {}
        "#,
        "/Main.sol",
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");
    state.vfs.write().set_file_contents(
        VfsPath::from(fixture.project_path("/Main.sol")),
        Some(Rope::from("import \"./Target.sol\";")),
    );

    let response =
        crate::handlers::goto_definition(&mut state, goto_params(uri, position)).await.unwrap();

    assert_eq!(response, None);
}

#[tokio::test(flavor = "current_thread")]
async fn changed_closed_import_does_not_use_an_old_code_symbol() {
    let fixture = RequestFixture::new(
        r#"
        //- /Main.sol
        contract $1Main {}

        //- /Target.sol
        contract Target {}
        "#,
        "/Main.sol",
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");
    std::fs::write(fixture.project_path("/Main.sol"), "import \"./Target.sol\";").unwrap();

    let response =
        crate::handlers::goto_definition(&mut state, goto_params(uri, position)).await.unwrap();

    let Some(GotoDefinitionResponse::Array(locations)) = response else {
        panic!("the current on-disk import should resolve");
    };
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri.to_file_path().unwrap(), fixture.project_path("/Target.sol"));
}

fn goto_params(uri: Url, position: Position) -> GotoDefinitionParams {
    GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri),
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}
