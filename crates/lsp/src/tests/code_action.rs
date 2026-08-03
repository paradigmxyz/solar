use crate::{
    code_actions::source_fingerprint, config::negotiate_capabilities_with_pull_diagnostic_data,
    global_state::GlobalState, test_support::TestProject,
};
use async_lsp::ClientSocket;
use lsp_types::{
    CodeActionClientCapabilities, CodeActionContext, CodeActionKind, CodeActionKindLiteralSupport,
    CodeActionLiteralSupport, CodeActionOrCommand, CodeActionParams, Diagnostic,
    DiagnosticSeverity, DocumentChanges, NumberOrString, PartialResultParams, Position,
    PublishDiagnosticsClientCapabilities, Range, TextDocumentIdentifier, TextEdit,
    WorkDoneProgressParams, WorkspaceEditClientCapabilities,
};
use std::{future::Future, sync::Arc};

#[test]
fn returns_native_quick_fix_with_legacy_changes() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let uri = lsp_types::Url::from_file_path(project.path("/Test.sol")).unwrap();
    let start = contents.find("bad_name").unwrap() as u32;
    let edit = TextEdit::new(
        Range::new(Position::new(0, start), Position::new(0, start + 8)),
        "badName".into(),
    );
    let diagnostic = Diagnostic {
        range: edit.range,
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(NumberOrString::String("mixed-case-variable".into())),
        code_description: None,
        source: Some("solar".into()),
        message: "mutable variables should use mixedCase".into(),
        related_information: None,
        tags: None,
        data: Some(serde_json::json!({
            "version": 1,
            "uri": uri,
            "sourceFingerprint": source_fingerprint(&contents),
            "suggestions": [{
                "title": "convert the name to mixedCase",
                "applicability": "MachineApplicable",
                "alternatives": [[edit]]
            }]
        })),
    };
    let params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: diagnostic.range,
        context: CodeActionContext { diagnostics: vec![diagnostic.clone()], ..Default::default() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "convert the name to mixedCase");
    assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
    assert_eq!(action.diagnostics.as_deref(), Some(std::slice::from_ref(&diagnostic)));
    assert_eq!(action.is_preferred, Some(true));
    let workspace_edit = action.edit.as_ref().expect("quick fix should have a workspace edit");
    assert_eq!(workspace_edit.changes.as_ref().unwrap()[&uri], [edit]);
    assert!(workspace_edit.document_changes.is_none());
}

#[test]
fn returns_multipart_suggestion_alternatives_with_utf16_ranges() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { string emoji = "😀"; uint256 bad_name; function bad_func() public {} }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let uri = lsp_types::Url::from_file_path(project.path("/Test.sol")).unwrap();
    let edit = |name: &str, replacement: &str| {
        let start = contents.find(name).unwrap();
        TextEdit::new(lsp_range(&contents, start, start + name.len()), replacement.into())
    };
    let first = vec![edit("bad_name", "badName"), edit("bad_func", "badFunc")];
    let second = vec![edit("bad_name", "goodName"), edit("bad_func", "goodFunc")];
    let diagnostic = Diagnostic {
        range: first[0].range,
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(NumberOrString::String("naming".into())),
        code_description: None,
        source: Some("solar".into()),
        message: "rename declarations".into(),
        related_information: None,
        tags: None,
        data: Some(serde_json::json!({
            "version": 1,
            "uri": uri,
            "sourceFingerprint": source_fingerprint(&contents),
            "suggestions": [{
                "title": "Rename declarations",
                "applicability": "MaybeIncorrect",
                "alternatives": [first, second]
            }]
        })),
    };
    let params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: diagnostic.range,
        context: CodeActionContext { diagnostics: vec![diagnostic], ..Default::default() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    assert_eq!(response.len(), 2);
    for (action, expected) in response.iter().zip([first, second]) {
        let CodeActionOrCommand::CodeAction(action) = action else {
            panic!("expected code action, got {action:#?}");
        };
        assert_eq!(action.title, "Rename declarations");
        assert_eq!(action.is_preferred, Some(false));
        assert_eq!(action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri], expected);
    }
}

#[test]
fn returns_current_open_document_version_when_supported() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol open
        contract Test { uint256 bad_name; }
        "#,
    );
    let (uri, edit, diagnostic, params) = native_request(&project);
    let mut state = state(&project, true);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.diagnostics.as_deref(), Some(std::slice::from_ref(&diagnostic)));
    let workspace_edit = action.edit.as_ref().unwrap();
    assert!(workspace_edit.changes.is_none());
    let Some(DocumentChanges::Edits(edits)) = &workspace_edit.document_changes else {
        panic!("expected versioned document edits");
    };
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].text_document.uri, uri);
    assert_eq!(edits[0].text_document.version, Some(0));
    assert_eq!(edits[0].edits, [lsp_types::OneOf::Left(edit)]);
}

#[test]
fn adds_suggested_function_mutability_for_solc_2018() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { function value() public returns (uint256) { return 1; } }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let function_start = contents.find("function").unwrap();
    let function_end = function_start + contents[function_start..].find(" }").unwrap() + " }".len();
    let insert = contents.find("returns").unwrap() as u32;
    let (uri, diagnostic, params) = fallback_request(
        &project,
        Range::new(Position::new(0, function_start as u32), Position::new(0, function_end as u32)),
        "flycheck",
        Some("2018"),
        "function state mutability can be restricted to view",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Change state mutability to `view`");
    assert_eq!(action.diagnostics.as_deref(), Some(std::slice::from_ref(&diagnostic)));
    assert_eq!(action.is_preferred, Some(true));
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(
            Range::new(Position::new(0, insert), Position::new(0, insert)),
            "view ".into(),
        )]
    );
}

#[test]
fn replaces_view_with_pure_for_solc_2018() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { function value() public view returns (uint256) { return 1; } }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let function_start = contents.find("function").unwrap();
    let function_end = function_start + contents[function_start..].find(" }").unwrap() + " }".len();
    let view_start = contents.find("view").unwrap() as u32;
    let (uri, _, params) = fallback_request(
        &project,
        Range::new(Position::new(0, function_start as u32), Position::new(0, function_end as u32)),
        "flycheck",
        Some("2018"),
        "Function state mutability can be restricted to pure.",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Change state mutability to `pure`");
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(
            Range::new(Position::new(0, view_start), Position::new(0, view_start + 4)),
            "pure".into(),
        )]
    );
}

#[test]
fn removes_uninitialized_unused_local_for_solc_2072() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test {
            function value() public pure returns (uint256) {
                uint256 unused;
                return 1;
            }
        }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let declaration_start = contents.find("uint256 unused").unwrap();
    let declaration_end = declaration_start + "uint256 unused".len();
    let line_start = contents[..declaration_start].rfind('\n').map_or(0, |position| position + 1);
    let line_end = contents[declaration_end..]
        .find('\n')
        .map_or(contents.len(), |position| declaration_end + position + 1);
    let (uri, _, params) = fallback_request(
        &project,
        lsp_range(&contents, declaration_start, declaration_end),
        "flycheck",
        Some("2072"),
        "Unused local variable.",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Remove unused local variable");
    assert_eq!(action.is_preferred, Some(true));
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(lsp_range(&contents, line_start, line_end), String::new())]
    );
}

#[test]
fn rejects_unsafe_unused_local_removals_for_solc_2072() {
    for source in [
        "contract Test { function f() public pure { uint256 unused = 1; } }",
        "contract Test { function f() public pure { for (uint256 unused; false;) {} } }",
    ] {
        let fixture = format!("//- /Test.sol\n{source}");
        let project = TestProject::from_fixture(&fixture);
        let contents = project.read_file("/Test.sol");
        let start = contents.find("uint256 unused").unwrap();
        let end = start + "uint256 unused".len();
        let (_, _, params) = fallback_request(
            &project,
            lsp_range(&contents, start, end),
            "flycheck",
            Some("2072"),
            "Unused local variable.",
        );
        let mut state = state(&project, false);

        assert!(authorized_code_actions(&mut state, params).is_empty(), "source: {source}");
    }
}

#[test]
fn adds_virtual_to_unimplemented_function_for_solc_5424() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { function value() public returns (uint256); }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let function_start = contents.find("function").unwrap();
    let function_end = contents.find(';').unwrap() + 1;
    let insert = contents.find("returns").unwrap() as u32;
    let (uri, _, params) = fallback_request(
        &project,
        Range::new(Position::new(0, function_start as u32), Position::new(0, function_end as u32)),
        "solar",
        Some("5424"),
        "functions without implementation must be marked virtual",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Add `virtual`");
    assert_eq!(action.is_preferred, Some(true));
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(
            Range::new(Position::new(0, insert), Position::new(0, insert)),
            "virtual ".into(),
        )]
    );
}

#[test]
fn adds_non_preferred_override_for_solc_9456() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { function value() public returns (uint256) { return 1; } }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let function_start = contents.find("function").unwrap();
    let function_end = function_start + contents[function_start..].find(" }").unwrap() + " }".len();
    let insert = contents.find("returns").unwrap() as u32;
    let (uri, _, mut params) = fallback_request(
        &project,
        Range::new(Position::new(0, function_start as u32), Position::new(0, function_end as u32)),
        "solar",
        Some("9456"),
        "overriding function is missing `override` specifier",
    );
    params.context.diagnostics.push(params.context.diagnostics[0].clone());
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Add `override`");
    assert_eq!(action.is_preferred, Some(false));
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(
            Range::new(Position::new(0, insert), Position::new(0, insert)),
            "override ".into(),
        )]
    );
}

#[test]
fn adds_override_to_fallback_and_receive_for_solc_9456() {
    for declaration in ["fallback() external {}", "receive() external payable {}"] {
        let fixture = format!("//- /Test.sol\ncontract Test {{ {declaration} }}");
        let project = TestProject::from_fixture(&fixture);
        let contents = project.read_file("/Test.sol");
        let function_start = contents.find(declaration).unwrap();
        let function_end = function_start + contents[function_start..].find('}').unwrap() + 1;
        let insert = function_start + contents[function_start..].find('{').unwrap();
        let (uri, _, params) = fallback_request(
            &project,
            lsp_range(&contents, function_start, function_end),
            "flycheck",
            Some("9456"),
            "Overriding function is missing \"override\" specifier.",
        );
        let mut state = state(&project, false);

        let response = authorized_code_actions(&mut state, params);

        let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
            panic!("expected one code action for {declaration}, got {response:#?}");
        };
        assert_eq!(action.title, "Add `override`");
        assert_eq!(action.is_preferred, Some(false));
        assert_eq!(
            action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
            [TextEdit::new(lsp_range(&contents, insert, insert), "override ".into())]
        );
    }
}

#[test]
fn adds_override_to_modifier_for_solc_9456() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { modifier onlyOwner() { _; } }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let modifier_start = contents.find("modifier").unwrap();
    let modifier_end = modifier_start + contents[modifier_start..].find(" }").unwrap() + " }".len();
    let insert = (modifier_start + contents[modifier_start..].find('{').unwrap()) as u32;
    let (uri, _, params) = fallback_request(
        &project,
        Range::new(Position::new(0, modifier_start as u32), Position::new(0, modifier_end as u32)),
        "solar",
        Some("9456"),
        "overriding modifier is missing `override` specifier",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Add `override`");
    assert_eq!(action.is_preferred, Some(false));
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(
            Range::new(Position::new(0, insert), Position::new(0, insert)),
            "override ".into(),
        )]
    );
}

#[test]
fn adds_override_to_public_variable_for_solc_9456() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 public value; }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let variable_start = contents.find("uint256").unwrap();
    let variable_end = contents.find(';').unwrap();
    let insert = contents.find("value").unwrap() as u32;
    let (uri, _, params) = fallback_request(
        &project,
        Range::new(Position::new(0, variable_start as u32), Position::new(0, variable_end as u32)),
        "solar",
        Some("9456"),
        "overriding public state variable is missing `override` specifier",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Add `override`");
    assert_eq!(action.is_preferred, Some(false));
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(
            Range::new(Position::new(0, insert), Position::new(0, insert)),
            "override ".into(),
        )]
    );
}

#[test]
fn offers_non_preferred_spdx_alternatives_for_solc_1878() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test {}
        "#,
    );
    let (uri, _, params) = fallback_request(
        &project,
        Range::default(),
        "flycheck",
        Some("1878"),
        "SPDX license identifier not provided in source file. Before publishing, consider adding a comment containing \"SPDX-License-Identifier: <SPDX-License>\" to each source file.",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    assert_eq!(response.len(), 2);
    for (action, license) in response.iter().zip(["MIT", "UNLICENSED"]) {
        let CodeActionOrCommand::CodeAction(action) = action else {
            panic!("expected code action, got {action:#?}");
        };
        assert_eq!(action.title, format!("Add `SPDX-License-Identifier: {license}`"));
        assert_eq!(action.is_preferred, Some(false));
        assert_eq!(
            action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
            [TextEdit::new(Range::default(), format!("// SPDX-License-Identifier: {license}\n"),)]
        );
    }
}

#[test]
fn offers_spdx_fallback_when_identifier_text_only_appears_in_a_string() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { string constant NOTICE = "SPDX-License-Identifier:"; }
        "#,
    );
    let (_, _, params) = fallback_request(
        &project,
        Range::default(),
        "flycheck",
        Some("1878"),
        "SPDX license identifier not provided in source file.",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    assert_eq!(response.len(), 2);
}

#[test]
fn adds_message_derived_pragma_for_solc_3420() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test {}
        "#,
    );
    project.write_file("/Test.sol", "contract Test {}\r\n");
    let (uri, _, params) = fallback_request(
        &project,
        Range::default(),
        "flycheck",
        Some("3420"),
        "Source file does not specify required compiler version! Consider adding \"pragma solidity ^0.8.99;\"",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Add `pragma solidity ^0.8.99;`");
    assert_eq!(action.is_preferred, Some(false));
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(Range::default(), "pragma solidity ^0.8.99;\r\n".into(),)]
    );
}

#[test]
fn offers_pragma_fallback_when_pragma_text_only_appears_in_a_comment() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        // TODO: add a pragma solidity directive after choosing a version.
        contract Test {}
        "#,
    );
    let (uri, _, params) = fallback_request(
        &project,
        Range::default(),
        "flycheck",
        Some("3420"),
        "Source file does not specify required compiler version! Consider adding \"pragma solidity ^0.8.99;\"",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one pragma code action, got {response:#?}");
    };
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(Range::default(), "pragma solidity ^0.8.99;\n".into())]
    );
}

#[test]
fn rejects_malformed_message_derived_pragma_for_solc_3420() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test {}
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let (_, diagnostic, params) = fallback_request(
        &project,
        Range::default(),
        "flycheck",
        Some("3420"),
        "Source file does not specify required compiler version! Consider adding \"pragma solidity 0.;\"",
    );

    let plans = crate::code_actions::plans(
        &params,
        std::slice::from_ref(&diagnostic),
        &crop::Rope::from(contents),
    );

    assert!(plans.is_empty());
}

#[test]
fn removes_whole_unused_import_statement() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        import "./Unused.sol" as Unused;
        contract Test {}
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let import_start = contents.find("import").unwrap();
    let import_end = contents.find(';').unwrap() + 1;
    let line_end = contents[import_end..].find('\n').map_or(import_end, |end| import_end + end + 1);
    let (uri, _, params) = fallback_request(
        &project,
        lsp_range(&contents, import_start, import_end),
        "solar",
        None,
        "unused import",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Remove unused import");
    assert_eq!(action.is_preferred, Some(true));
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(lsp_range(&contents, import_start, line_end), String::new())]
    );
}

#[test]
fn removes_forge_lint_unused_import_without_structured_suggestion() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        import "./Unused.sol" as Unused;
        contract Test {}
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let import_start = contents.find("import").unwrap();
    let import_end = contents.find(';').unwrap() + 1;
    let line_end = contents[import_end..].find('\n').map_or(import_end, |end| import_end + end + 1);
    let (uri, _, params) = fallback_request(
        &project,
        lsp_range(&contents, import_start, import_end),
        "forge-lint",
        Some("unused-import"),
        "unused imports should be removed",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Remove unused import");
    assert_eq!(action.is_preferred, Some(true));
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(lsp_range(&contents, import_start, line_end), String::new())]
    );
}

#[test]
fn rejects_whole_item_range_for_named_unused_import() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        import {Unused, Used} from "./Types.sol";
        contract Test { Used value; }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let import_start = contents.find("import").unwrap();
    let import_end = contents.find(';').unwrap() + 1;
    let (_, diagnostic, params) = fallback_request(
        &project,
        lsp_range(&contents, import_start, import_end),
        "solar",
        None,
        "unused import",
    );

    let plans = crate::code_actions::plans(
        &params,
        std::slice::from_ref(&diagnostic),
        &crop::Rope::from(contents),
    );

    assert!(plans.is_empty());
}

#[test]
fn removes_named_unused_import_with_adjacent_comma() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        import {Unused, Used} from "./Types.sol";
        contract Test { Used value; }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let unused_start = contents.find("Unused").unwrap();
    let unused_end = unused_start + "Unused".len();
    let used_start = unused_end + contents[unused_end..].find("Used").unwrap();
    let (uri, _, params) = fallback_request(
        &project,
        lsp_range(&contents, unused_start, unused_end),
        "solar",
        None,
        "unused import",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.title, "Remove unused import");
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(lsp_range(&contents, unused_start, used_start), String::new())]
    );
}

#[test]
fn removes_last_named_unused_import_with_preceding_comma() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        import {Used, Unused as Alias} from "./Types.sol";
        contract Test { Used value; }
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let used_start = contents.find("Used").unwrap();
    let used_end = used_start + "Used".len();
    let unused_start = used_end + contents[used_end..].find("Unused").unwrap();
    let alias_end = contents.find("Alias").unwrap() + "Alias".len();
    let (uri, _, params) = fallback_request(
        &project,
        lsp_range(&contents, unused_start, alias_end),
        "solar",
        None,
        "unused import",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(lsp_range(&contents, used_end, alias_end), String::new())]
    );
}

#[test]
fn removes_whole_import_for_sole_unused_named_binding() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        import {Unused as Alias} from "./Types.sol";
        contract Test {}
        "#,
    );
    let contents = project.read_file("/Test.sol");
    let import_start = contents.find("import").unwrap();
    let import_end = contents.find(';').unwrap() + 1;
    let line_end = import_end + contents[import_end..].find('\n').map_or(0, |end| end + 1);
    let unused_start = contents.find("Unused").unwrap();
    let alias_end = contents.find("Alias").unwrap() + "Alias".len();
    let (uri, _, params) = fallback_request(
        &project,
        lsp_range(&contents, unused_start, alias_end),
        "solar",
        None,
        "unused import",
    );
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(
        action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri],
        [TextEdit::new(lsp_range(&contents, import_start, line_end), String::new())]
    );
}

#[test]
fn honors_requested_code_action_kinds() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (_, _, _, mut params) = native_request(&project);
    params.context.only = Some(vec![CodeActionKind::SOURCE]);
    let mut state = state(&project, false);

    let response = authorized_code_actions(&mut state, params);

    assert!(response.is_empty());
}

#[test]
fn requires_the_request_range_to_intersect_the_diagnostic() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (_, _, _, mut params) = native_request(&project);
    let mut state = state(&project, false);

    params.range = Range::default();
    assert!(authorized_code_actions(&mut state, params.clone()).is_empty());

    params.range = Range::new(Position::new(0, u32::MAX), Position::new(0, u32::MAX));
    assert!(authorized_code_actions(&mut state, params).is_empty());
}

#[test]
fn accepts_cursor_and_selection_ranges_intersecting_the_diagnostic() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (_, _, diagnostic, mut params) = native_request(&project);
    let mut state = state(&project, false);
    let start = diagnostic.range.start.character;

    params.range = Range::new(Position::new(0, start + 1), Position::new(0, start + 1));
    assert_eq!(authorized_code_actions(&mut state, params.clone()).len(), 1);

    params.range = Range::new(Position::new(0, start - 1), Position::new(0, start + 1));
    assert_eq!(authorized_code_actions(&mut state, params).len(), 1);
}

#[test]
fn rejects_stale_disk_and_open_document_fingerprints() {
    let disk = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (_, _, _, disk_params) = native_request(&disk);
    disk.write_file("/Test.sol", "contract Test { uint256 changed; }");
    let mut disk_state = state(&disk, false);
    let response = authorized_code_actions(&mut disk_state, disk_params);
    assert!(response.is_empty());

    let open = TestProject::from_fixture(
        r#"
        //- /Test.sol open
        contract Test { uint256 bad_name; }
        "#,
    );
    let (uri, _, _, open_params) = native_request(&open);
    let mut open_state = state(&open, false);
    open_state.vfs.write().set_file_contents_with_version(
        crate::proto::vfs_path(&uri).unwrap(),
        Some(crop::Rope::from("contract Test { uint256 changed; }")),
        Some(1),
    );
    let response = authorized_code_actions(&mut open_state, open_params);
    assert!(response.is_empty());
}

#[test]
fn canonicalizes_equivalent_file_uris_before_validating_diagnostic_data() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (uri, edit, diagnostic, mut params) = native_request(&project);
    let encoded =
        lsp_types::Url::parse(&uri.as_str().replacen("Test.sol", "%54est.sol", 1)).unwrap();
    assert_ne!(uri, encoded);
    assert_eq!(uri.to_file_path(), encoded.to_file_path());
    params.text_document.uri = encoded;
    let mut state = state(&project, false);
    replace_diagnostics(&state, uri.clone(), vec![diagnostic]);

    let response = block_on(crate::handlers::code_actions(&mut state, params)).unwrap().unwrap();

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri], [edit]);
}

#[test]
fn rejects_diagnostic_that_is_not_in_the_current_server_report() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (_, _, _, params) = native_request(&project);
    let mut state = state(&project, false);

    let response = block_on(crate::handlers::code_actions(&mut state, params)).unwrap().unwrap();

    assert!(response.is_empty());
}

#[test]
fn uses_current_server_diagnostics_when_client_context_is_empty() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (uri, edit, diagnostic, mut params) = native_request(&project);
    params.context.diagnostics.clear();
    let mut state = state(&project, false);
    replace_diagnostics(&state, uri.clone(), vec![diagnostic.clone()]);

    let response = block_on(crate::handlers::code_actions(&mut state, params)).unwrap().unwrap();

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one server-owned code action, got {response:#?}");
    };
    assert_eq!(action.diagnostics.as_deref(), Some(std::slice::from_ref(&diagnostic)));
    assert_eq!(action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri], [edit]);
}

#[test]
fn uses_current_server_diagnostic_when_client_presentation_is_stale() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (uri, edit, diagnostic, mut params) = native_request(&project);
    params.context.diagnostics[0].message = "stale client message".into();
    params.context.diagnostics[0].severity = Some(DiagnosticSeverity::ERROR);
    let mut state = state(&project, false);
    replace_diagnostics(&state, uri.clone(), vec![diagnostic.clone()]);

    let response = block_on(crate::handlers::code_actions(&mut state, params)).unwrap().unwrap();

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one server-owned code action, got {response:#?}");
    };
    assert_eq!(action.diagnostics.as_deref(), Some(std::slice::from_ref(&diagnostic)));
    assert_eq!(action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri], [edit]);
}

#[test]
fn uses_all_server_diagnostics_when_client_context_is_incomplete() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (uri, _, diagnostic, params) = native_request(&project);
    let mut omitted = diagnostic.clone();
    omitted.code = Some(NumberOrString::String("different-lint".into()));
    omitted.message = "a different server diagnostic".into();
    let omitted_data = omitted.data.as_mut().unwrap();
    omitted_data["suggestions"][0]["title"] = serde_json::json!("apply different fix");
    omitted_data["suggestions"][0]["alternatives"][0][0]["newText"] =
        serde_json::json!("differentName");
    let mut state = state(&project, false);
    replace_diagnostics(&state, uri, vec![diagnostic, omitted]);

    let response = block_on(crate::handlers::code_actions(&mut state, params)).unwrap().unwrap();

    assert_eq!(response.len(), 2, "expected actions for every current server diagnostic");
}

#[test]
fn returns_no_literal_action_when_the_client_did_not_advertise_support() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (uri, _, diagnostic, params) = native_request(&project);
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    *state.vfs.write() = project.vfs();
    replace_diagnostics(&state, uri, vec![diagnostic]);

    let response = block_on(crate::handlers::code_actions(&mut state, params)).unwrap().unwrap();

    assert!(response.is_empty());
}

#[test]
fn omits_optional_fields_but_keeps_server_owned_fix_data() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (uri, edit, diagnostic, mut params) = native_request(&project);
    params.context.diagnostics[0].data = None;
    let mut state = state_with_capabilities(&project, false, false, false);
    replace_diagnostics(&state, uri.clone(), vec![diagnostic]);

    let report = block_on(state.pull_diagnostic_report(uri.clone(), None)).unwrap();
    let crate::diagnostics::PullReport::Full { diagnostics, .. } = report else {
        panic!("expected a full diagnostic report");
    };
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].data.is_none());

    let response = block_on(crate::handlers::code_actions(&mut state, params)).unwrap().unwrap();

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert_eq!(action.is_preferred, None);
    assert!(action.diagnostics.as_ref().unwrap()[0].data.is_none());
    assert_eq!(action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri], [edit]);
}

#[test]
fn pull_only_diagnostic_data_support_preserves_quick_fixes() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (uri, edit, diagnostic, mut params) = native_request(&project);
    let mut state = state_with_diagnostic_capabilities(&project, false, true, false, true);
    replace_diagnostics(&state, uri.clone(), vec![diagnostic]);

    let report = block_on(state.pull_diagnostic_report(uri.clone(), None)).unwrap();
    let crate::diagnostics::PullReport::Full { diagnostics, .. } = report else {
        panic!("expected a full diagnostic report");
    };
    assert!(diagnostics[0].data.is_some());
    params.context.diagnostics = diagnostics;

    let response = block_on(crate::handlers::code_actions(&mut state, params)).unwrap().unwrap();

    let [CodeActionOrCommand::CodeAction(action)] = response.as_slice() else {
        panic!("expected one code action, got {response:#?}");
    };
    assert!(action.diagnostics.as_ref().unwrap()[0].data.is_some());
    assert_eq!(action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri], [edit]);
}

#[test]
fn uses_server_owned_data_when_the_client_changes_or_omits_it() {
    let project = TestProject::from_fixture(
        r#"
        //- /Test.sol
        contract Test { uint256 bad_name; }
        "#,
    );
    let (uri, edit, diagnostic, mut changed_params) = native_request(&project);
    changed_params.context.diagnostics[0].data.as_mut().unwrap()["suggestions"][0]["alternatives"]
        [0][0]["newText"] = serde_json::json!("clientControlled");
    let mut state = state(&project, false);
    replace_diagnostics(&state, uri.clone(), vec![diagnostic.clone()]);

    let changed =
        block_on(crate::handlers::code_actions(&mut state, changed_params)).unwrap().unwrap();

    let [CodeActionOrCommand::CodeAction(action)] = changed.as_slice() else {
        panic!("expected one server-authorized code action, got {changed:#?}");
    };
    assert_eq!(action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri], [edit]);
    assert_eq!(action.diagnostics.as_deref(), Some(std::slice::from_ref(&diagnostic)));

    let (_, _, _, mut missing_params) = native_request(&project);
    missing_params.context.diagnostics[0].data = None;
    let missing =
        block_on(crate::handlers::code_actions(&mut state, missing_params)).unwrap().unwrap();
    assert_eq!(missing.len(), 1);
}

fn native_request(
    project: &TestProject,
) -> (lsp_types::Url, TextEdit, Diagnostic, CodeActionParams) {
    let contents = project.read_file("/Test.sol");
    let uri = lsp_types::Url::from_file_path(project.path("/Test.sol")).unwrap();
    let start = contents.find("bad_name").unwrap() as u32;
    let edit = TextEdit::new(
        Range::new(Position::new(0, start), Position::new(0, start + 8)),
        "badName".into(),
    );
    let diagnostic = Diagnostic {
        range: edit.range,
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(NumberOrString::String("mixed-case-variable".into())),
        code_description: None,
        source: Some("solar".into()),
        message: "mutable variables should use mixedCase".into(),
        related_information: None,
        tags: None,
        data: Some(serde_json::json!({
            "version": 1,
            "uri": uri,
            "sourceFingerprint": source_fingerprint(&contents),
            "suggestions": [{
                "title": "convert the name to mixedCase",
                "applicability": "MachineApplicable",
                "alternatives": [[edit]]
            }]
        })),
    };
    let params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: diagnostic.range,
        context: CodeActionContext { diagnostics: vec![diagnostic.clone()], ..Default::default() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    (uri, edit, diagnostic, params)
}

fn fallback_request(
    project: &TestProject,
    range: Range,
    source: &str,
    code: Option<&str>,
    message: &str,
) -> (lsp_types::Url, Diagnostic, CodeActionParams) {
    let contents = project.read_file("/Test.sol");
    let uri = lsp_types::Url::from_file_path(project.path("/Test.sol")).unwrap();
    let diagnostic = Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::WARNING),
        code: code.map(|code| NumberOrString::String(code.into())),
        code_description: None,
        source: Some(source.into()),
        message: message.into(),
        related_information: None,
        tags: None,
        data: Some(serde_json::json!({
            "version": 1,
            "uri": uri,
            "sourceFingerprint": source_fingerprint(&contents),
            "suggestions": []
        })),
    };
    let params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range,
        context: CodeActionContext { diagnostics: vec![diagnostic.clone()], ..Default::default() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    (uri, diagnostic, params)
}

fn lsp_range(contents: &str, start: usize, end: usize) -> Range {
    let contents = crop::Rope::from(contents);
    Range::new(
        crate::proto::position_at_byte(&contents, start).unwrap(),
        crate::proto::position_at_byte(&contents, end).unwrap(),
    )
}

fn state(project: &TestProject, document_changes: bool) -> GlobalState {
    state_with_capabilities(project, document_changes, true, true)
}

fn state_with_capabilities(
    project: &TestProject,
    document_changes: bool,
    is_preferred: bool,
    diagnostic_data: bool,
) -> GlobalState {
    state_with_diagnostic_capabilities(
        project,
        document_changes,
        is_preferred,
        diagnostic_data,
        diagnostic_data,
    )
}

fn state_with_diagnostic_capabilities(
    project: &TestProject,
    document_changes: bool,
    is_preferred: bool,
    publish_diagnostic_data: bool,
    pull_diagnostic_data: bool,
) -> GlobalState {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let mut initialize = project.initialize_params();
    let text_document = initialize.capabilities.text_document.get_or_insert_default();
    text_document.code_action = Some(CodeActionClientCapabilities {
        code_action_literal_support: Some(CodeActionLiteralSupport {
            code_action_kind: CodeActionKindLiteralSupport {
                value_set: vec![CodeActionKind::QUICKFIX.as_str().into()],
            },
        }),
        is_preferred_support: Some(is_preferred),
        ..Default::default()
    });
    text_document.publish_diagnostics = Some(PublishDiagnosticsClientCapabilities {
        data_support: Some(publish_diagnostic_data),
        ..Default::default()
    });
    if document_changes {
        initialize.capabilities.workspace.get_or_insert_default().workspace_edit =
            Some(WorkspaceEditClientCapabilities {
                document_changes: Some(true),
                ..Default::default()
            });
    }
    let config =
        negotiate_capabilities_with_pull_diagnostic_data(initialize, pull_diagnostic_data).1;
    state.config = Arc::new(config);
    *state.vfs.write() = project.vfs();
    state
}

fn authorized_code_actions(
    state: &mut GlobalState,
    params: CodeActionParams,
) -> Vec<CodeActionOrCommand> {
    replace_diagnostics(
        state,
        params.text_document.uri.clone(),
        params.context.diagnostics.clone(),
    );
    block_on(crate::handlers::code_actions(state, params)).unwrap().unwrap()
}

fn replace_diagnostics(state: &GlobalState, uri: lsp_types::Url, diagnostics: Vec<Diagnostic>) {
    let mut diagnostic_map = crate::diagnostics::DiagnosticMap::default();
    diagnostic_map.insert(uri, diagnostics);
    state.replace_diagnostics_for_test(diagnostic_map);
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
}
