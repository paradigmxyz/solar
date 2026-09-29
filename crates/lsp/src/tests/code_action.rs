use crate::{
    LaunchConfig,
    code_actions::{DiagnosticData, DiagnosticSuggestion, source_fingerprint},
    config::negotiate_capabilities_with_pull_diagnostic_data,
    global_state::GlobalState,
    test_support::TestProject,
};
use async_lsp::ClientSocket;
use lsp_types::{
    CodeActionClientCapabilities, CodeActionContext, CodeActionKind, CodeActionKindLiteralSupport,
    CodeActionLiteralSupport, CodeActionOrCommand, CodeActionParams, Diagnostic,
    DiagnosticClientCapabilities, DiagnosticSeverity, DiagnosticWorkspaceClientCapabilities,
    DocumentChanges, NumberOrString, OneOf, PartialResultParams, Position,
    PublishDiagnosticsClientCapabilities, Range, TextDocumentIdentifier, TextEdit, Url,
    WorkDoneProgressParams, WorkspaceEditClientCapabilities,
};
use serde_json::json;
use snapbox::{IntoData, assert_data_eq, str};
use solar_interface::diagnostics::Applicability;
use std::{fmt::Write as _, future::Future, sync::Arc};

const BAD_NAME: &str = "contract Test { uint256 bad_name; }\n";

/// A fallback diagnostic as `(source file, diagnostic range text, origin, code, message)`.
type FallbackCase<'a> = (&'a str, &'a str, &'a str, Option<&'a str>, &'a str);

#[test]
fn returns_native_quick_fix_as_legacy_or_versioned_edits() {
    let project = TestProject::from_fixture(&format!("//- /Test.sol open\n{BAD_NAME}"));
    let (_, _, diagnostic, params) = native_request(&project);
    for (document_changes, expected) in [
        (
            false,
            str![[r#"
convert the name to mixedCase preferred=Some(true)
  Test.sol 0:24-0:32 "badName"

"#]],
        ),
        (
            true,
            str![[r#"
convert the name to mixedCase preferred=Some(true)
  Test.sol@Some(0) 0:24-0:32 "badName"

"#]],
        ),
    ] {
        let response =
            authorized_code_actions(&mut state(&project, document_changes), params.clone());
        assert_eq!(action_diagnostics(&response), std::slice::from_ref(&diagnostic));
        assert_data_eq!(actions_output(&response), expected);
    }
}

#[test]
fn returns_multipart_suggestion_alternatives_with_utf16_ranges() {
    let project = TestProject::new();
    let contents =
        "contract Test { string emoji = \"😀\"; uint256 bad_name; function bad_func() public {} }";
    project.write_file("/Test.sol", contents);
    let edit = |name: &str, replacement: &str| {
        let start = contents.find(name).unwrap();
        TextEdit::new(lsp_range(contents, start, start + name.len()), replacement.into())
    };
    let first = vec![edit("bad_name", "badName"), edit("bad_func", "badFunc")];
    let second = vec![edit("bad_name", "goodName"), edit("bad_func", "goodFunc")];
    let (_, mut diagnostic, mut params) =
        fallback_request(&project, first[0].range, "solar", Some("naming"), "rename declarations");
    set_suggestion(
        &mut diagnostic,
        "Rename declarations",
        "MaybeIncorrect",
        json!([first, second]),
    );
    params.context.diagnostics = vec![diagnostic];

    let response = authorized_code_actions(&mut state(&project, false), params);
    assert_data_eq!(
        actions_output(&response),
        str![[r#"
Rename declarations preferred=Some(false)
  Test.sol 0:45-0:53 "badName"
  Test.sol 0:64-0:72 "badFunc"
Rename declarations preferred=Some(false)
  Test.sol 0:45-0:53 "goodName"
  Test.sol 0:64-0:72 "goodFunc"

"#]]
    );
}

#[test]
fn mixed_quick_fixes_share_current_syntax_and_preserve_order() {
    for eol in ["\n", "\r\n", "\r"] {
        let mut project = TestProject::new();
        let first = "function first() public returns (uint256) { return 1; }";
        let second = "function second() public view returns (uint256) { return 2; }";
        let contents = ["// 😀", "contract Test {", first, second, "}", ""].join(eol);
        project.write_file("/Test.sol", &contents);
        project.open_file("/Test.sol", &contents);
        let range_of = |text: &str| {
            let start = contents.find(text).unwrap();
            lsp_range(&contents, start, start + text.len())
        };
        let (uri, first_diagnostic, mut params) = fallback_request(
            &project,
            range_of(first),
            "solar",
            Some("2018"),
            "function state mutability can be restricted to pure",
        );
        let (_, second_diagnostic, _) = fallback_request(
            &project,
            range_of(second),
            "flycheck",
            Some("2018"),
            "Function state mutability can be restricted to pure.",
        );
        let native_edit = TextEdit::new(range_of("Test"), "Renamed".into());
        let mut native = first_diagnostic.clone();
        native.range = native_edit.range;
        native.message = "rename contract".into();
        native.data = Some(native_data(uri, &contents, "Rename contract", native_edit));
        params.range = lsp_range(&contents, 0, contents.len());
        params.context.diagnostics = vec![first_diagnostic, native, second_diagnostic];
        let response = authorized_code_actions(&mut state(&project, true), params);
        assert_data_eq!(
            actions_output(&response),
            str![[r#"
Change state mutability to `pure` preferred=Some(true)
  Test.sol@Some(0) 2:24-2:24 "pure "
Rename contract preferred=Some(true)
  Test.sol@Some(0) 1:9-1:13 "Renamed"
Change state mutability to `pure` preferred=Some(true)
  Test.sol@Some(0) 3:25-3:29 "pure"

"#]]
        );
    }
}

#[test]
fn failed_fallback_parse_preserves_native_suggestions() {
    let project = TestProject::new();
    let contents = "contract Test { function first() public { uint256 broken = ;\n";
    project.write_file("/Test.sol", contents);
    let (uri, fallback, mut params) = fallback_request(
        &project,
        lsp_range(contents, contents.find("function").unwrap(), contents.len()),
        "solar",
        Some("2018"),
        "function state mutability can be restricted to pure",
    );
    let edit = TextEdit::new(lsp_range(contents, contents.len(), contents.len()), "} }".into());
    let mut native = fallback.clone();
    native.message = "close blocks".into();
    native.data = Some(native_data(uri, contents, "Close blocks", edit));
    params.range = lsp_range(contents, 0, contents.len());
    params.context.diagnostics = vec![fallback.clone(), native, fallback];
    let response = authorized_code_actions(&mut state(&project, false), params);
    assert_data_eq!(
        actions_output(&response),
        str![[r#"
Close blocks preferred=Some(true)
  Test.sol 1:0-1:0 "} }"

"#]]
    );
}

#[test]
fn changes_function_mutability_for_solc_2018() {
    check_fallbacks(
        &[
            (
                "contract Test { function value() public returns (uint256) { return 1; } }",
                "function value() public returns (uint256) { return 1; }",
                "flycheck",
                Some("2018"),
                "function state mutability can be restricted to view",
            ),
            (
                "contract Test { function value() public view returns (uint256) { return 1; } }",
                "function value() public view returns (uint256) { return 1; }",
                "flycheck",
                Some("2018"),
                "Function state mutability can be restricted to pure.",
            ),
        ],
        str![[r#"
== function value() public returns (uint256) { return 1; }
Change state mutability to `view` preferred=Some(true)
  Test.sol 0:40-0:40 "view "
== function value() public view returns (uint256) { return 1; }
Change state mutability to `pure` preferred=Some(true)
  Test.sol 0:40-0:44 "pure"

"#]],
    );
}

#[test]
fn removes_only_uninitialized_unused_locals_for_solc_2072() {
    // Presentation details after the primary message do not hide the fix.
    let message = "Unused local variable.\nnote: the declaration is never read";
    let unused = |source| (source, "uint256 unused", "flycheck", Some("2072"), message);
    check_fallbacks(
        &[
            unused(
                "contract Test {\n    function value() public pure returns (uint256) {\n        \
                 uint256 unused;\n        return 1;\n    }\n}\n",
            ),
            // A statement that shares its line keeps the surrounding text.
            unused("contract Test { function f() public pure { uint256 unused; } }"),
            unused("contract Test { function f() public pure { uint256 unused = 1; } }"),
            unused(
                "contract Test { function f() public pure { for (uint256 unused; false;) {} } }",
            ),
        ],
        str![[r#"
== uint256 unused
Remove unused local variable preferred=Some(true)
  Test.sol 2:0-3:0 ""
== uint256 unused
Remove unused local variable preferred=Some(true)
  Test.sol 0:43-0:58 ""
== uint256 unused
== uint256 unused

"#]],
    );
}

#[test]
fn adds_virtual_to_unimplemented_function_for_solc_5424() {
    let source = "contract Test { function value() public returns (uint256); }";
    let message = "functions without implementation must be marked virtual";
    let target = "function value() public returns (uint256);";
    check_fallbacks(
        // Fallback fixes require a diagnostic code.
        &[
            (source, target, "solar", Some("5424"), message),
            (source, target, "solar", None, message),
        ],
        str![[r#"
== function value() public returns (uint256);
Add `virtual` preferred=Some(true)
  Test.sol 0:40-0:40 "virtual "
== function value() public returns (uint256);

"#]],
    );
}

#[test]
fn adds_non_preferred_override_for_solc_9456() {
    let solc = "Overriding function is missing \"override\" specifier.";
    check_fallbacks(
        &[
            (
                "contract Test { function value() public returns (uint256) { return 1; } }",
                "function value() public returns (uint256) { return 1; }",
                "solar",
                Some("9456"),
                "overriding function is missing `override` specifier",
            ),
            (
                "contract Test { fallback() external {} }",
                "fallback() external {}",
                "flycheck",
                Some("9456"),
                solc,
            ),
            (
                "contract Test { receive() external payable {} }",
                "receive() external payable {}",
                "flycheck",
                Some("9456"),
                solc,
            ),
            (
                "contract Test { modifier onlyOwner() { _; } }",
                "modifier onlyOwner() { _; }",
                "solar",
                Some("9456"),
                "overriding modifier is missing `override` specifier",
            ),
            (
                "contract Test { uint256 public value; }",
                "uint256 public value",
                "solar",
                Some("9456"),
                "overriding public state variable is missing `override` specifier",
            ),
        ],
        str![[r#"
== function value() public returns (uint256) { return 1; }
Add `override` preferred=Some(false)
  Test.sol 0:40-0:40 "override "
== fallback() external {}
Add `override` preferred=Some(false)
  Test.sol 0:36-0:36 "override "
== receive() external payable {}
Add `override` preferred=Some(false)
  Test.sol 0:43-0:43 "override "
== modifier onlyOwner() { _; }
Add `override` preferred=Some(false)
  Test.sol 0:37-0:37 "override "
== uint256 public value
Add `override` preferred=Some(false)
  Test.sol 0:31-0:31 "override "

"#]],
    );
}

#[test]
fn offers_non_preferred_spdx_alternatives_for_solc_1878() {
    let message = "SPDX license identifier not provided in source file. Before publishing, consider adding a comment containing \"SPDX-License-Identifier: <SPDX-License>\" to each source file.";
    check_fallbacks(
        &[
            ("contract Test {}\n", "", "flycheck", Some("1878"), message),
            (
                "contract Test { string constant NOTICE = \"SPDX-License-Identifier:\"; }\n",
                "",
                "flycheck",
                Some("1878"),
                "SPDX license identifier not provided in source file.",
            ),
        ],
        str![[r#"
==
Add `SPDX-License-Identifier: MIT` preferred=Some(false)
  Test.sol 0:0-0:0 "// SPDX-License-Identifier: MIT\n"
Add `SPDX-License-Identifier: UNLICENSED` preferred=Some(false)
  Test.sol 0:0-0:0 "// SPDX-License-Identifier: UNLICENSED\n"
==
Add `SPDX-License-Identifier: MIT` preferred=Some(false)
  Test.sol 0:0-0:0 "// SPDX-License-Identifier: MIT\n"
Add `SPDX-License-Identifier: UNLICENSED` preferred=Some(false)
  Test.sol 0:0-0:0 "// SPDX-License-Identifier: UNLICENSED\n"

"#]],
    );
}

#[test]
fn adds_message_derived_pragma_for_solc_3420() {
    let pragma = |source, version| {
        let message = format!(
            "Source file does not specify required compiler version! Consider adding \"pragma solidity {version};\""
        );
        (source, message)
    };
    let cases = [
        pragma("contract Test {}\r\n", "^0.8.99"),
        pragma(
            "// TODO: add a pragma solidity directive after choosing a version.\ncontract Test {}\n",
            "^0.8.99",
        ),
        pragma("contract Test {}\n", "0."),
    ];
    check_fallbacks(
        &cases
            .each_ref()
            .map(|(source, message)| (*source, "", "flycheck", Some("3420"), message.as_str())),
        str![[r#"
==
Add `pragma solidity ^0.8.99;` preferred=Some(false)
  Test.sol 0:0-0:0 "pragma solidity ^0.8.99;\r\n"
==
Add `pragma solidity ^0.8.99;` preferred=Some(false)
  Test.sol 0:0-0:0 "pragma solidity ^0.8.99;\n"
==

"#]],
    );
}

#[test]
fn removes_unused_imports() {
    let solar = |source, target| (source, target, "solar", None, "unused import");
    let plain = "import \"./Unused.sol\" as Unused;\ncontract Test {}\n";
    let named = "import {Unused, Used} from \"./Types.sol\";\ncontract Test { Used value; }\n";
    check_fallbacks(
        &[
            solar(plain, "import \"./Unused.sol\" as Unused;"),
            (
                plain,
                "import \"./Unused.sol\" as Unused;",
                "forge-lint",
                Some("unused-import"),
                "unused imports should be removed",
            ),
            solar(named, "import {Unused, Used} from \"./Types.sol\";"),
            solar(named, "Unused"),
            solar(
                "import {Used, Unused as Alias} from \"./Types.sol\";\ncontract Test { Used value; }\n",
                "Unused as Alias",
            ),
            solar(
                "import {Unused as Alias} from \"./Types.sol\";\ncontract Test {}\n",
                "Unused as Alias",
            ),
        ],
        str![[r#"
== import "./Unused.sol" as Unused;
Remove unused import preferred=Some(true)
  Test.sol 0:0-1:0 ""
== import "./Unused.sol" as Unused;
Remove unused import preferred=Some(true)
  Test.sol 0:0-1:0 ""
== import {Unused, Used} from "./Types.sol";
== Unused
Remove unused import preferred=Some(true)
  Test.sol 0:8-0:16 ""
== Unused as Alias
Remove unused import preferred=Some(true)
  Test.sol 0:12-0:29 ""
== Unused as Alias
Remove unused import preferred=Some(true)
  Test.sol 0:0-1:0 ""

"#]],
    );
}

#[test]
fn filters_by_requested_kind_and_range() {
    let project = TestProject::from_fixture(&format!("//- /Test.sol\n{BAD_NAME}"));
    let (_, _, diagnostic, mut params) = native_request(&project);
    let mut state = state(&project, false);
    let Range { start, end } = diagnostic.range;
    let before = Position::new(0, start.character - 1);
    let inside = Position::new(0, start.character + 1);
    let past_end = Position::new(0, u32::MAX);
    for (only, range, actions) in [
        (Some(CodeActionKind::SOURCE), diagnostic.range, 0),
        (Some(CodeActionKind::QUICKFIX), diagnostic.range, 1),
        (None, Range::default(), 0),
        (None, Range::new(past_end, past_end), 0),
        (None, Range::new(start, start), 1),
        (None, Range::new(inside, inside), 1),
        (None, Range::new(end, end), 1),
        (None, Range::new(before, inside), 1),
    ] {
        params.context.only = only.map(|kind| vec![kind]);
        params.range = range;
        assert_eq!(authorized_code_actions(&mut state, params.clone()).len(), actions, "{range:?}");
    }
}

#[test]
fn range_filtering_preserves_server_order_empty_ranges_and_data_disambiguation() {
    for eol in ["\n", "\r\n", "\r"] {
        let mut project = TestProject::new();
        let contents = [
            "contract Test {",
            "string constant face = unicode\"😀\"; uint256 bad_name;",
            "uint256 other_name;",
            "}",
            "",
        ]
        .join(eol);
        project.write_file("/Test.sol", &contents);
        project.open_file("/Test.sol", &contents);
        let start = contents.find("bad_name").unwrap();
        let range = lsp_range(&contents, start, start + "bad_name".len());
        let (uri, diagnostic, mut params) =
            fallback_request(&project, range, "solar", Some("naming"), "rename declaration");
        let make_diagnostic = |range: Range, title: &str| {
            let mut diagnostic = diagnostic.clone();
            diagnostic.range = range;
            let alternatives = json!([[TextEdit::new(range, title.into())]]);
            set_suggestion(&mut diagnostic, title, "MachineApplicable", alternatives);
            diagnostic
        };
        let diagnostics = vec![
            make_diagnostic(Range::new(range.end, range.end), "end insertion"),
            make_diagnostic(lsp_range(&contents, 0, "contract".len()), "before selection"),
            make_diagnostic(range, "alternate name"),
            make_diagnostic(range, "preferred name"),
            make_diagnostic(Range::new(range.start, range.start), "start insertion"),
            make_diagnostic(
                lsp_range(&contents, start + "bad_name".len(), start + "bad_name;".len()),
                "touching selection end",
            ),
            make_diagnostic(lsp_range(&contents, start - 1, start), "touching selection start"),
            make_diagnostic(
                lsp_range(&contents, 0, contents.find("other_name").unwrap()),
                "spanning selection",
            ),
        ];
        let mut state = state(&project, false);
        replace_diagnostics(&state, uri.clone(), diagnostics.clone());
        let selected = block_on(state.code_action_diagnostics(uri, range)).unwrap();
        assert_eq!(selected, [0, 2, 3, 4, 7].map(|index| diagnostics[index].clone()), "{eol:?}");

        for (context, expected) in
            [(Vec::new(), vec![0, 2, 3, 4, 7]), (vec![diagnostics[3].clone()], vec![0, 3, 4, 7])]
        {
            params.context.diagnostics = context;
            let response = code_actions(&mut state, params.clone());
            let expected = expected.into_iter().map(|index| diagnostics[index].clone());
            assert_eq!(action_diagnostics(&response), expected.collect::<Vec<_>>(), "{eol:?}");
        }
    }
}

#[test]
fn range_filtering_keeps_exact_position_validation() {
    let mut project = TestProject::new();
    let contents = "contract Test {\r\nstring constant face = unicode\"😀\";\r\n}\r\n";
    project.write_file("/Test.sol", contents);
    project.open_file("/Test.sol", contents);
    let emoji = contents.find('😀').unwrap();
    let range = lsp_range(contents, emoji, emoji + '😀'.len_utf8());
    let (uri, mut diagnostic, mut params) =
        fallback_request(&project, range, "solar", Some("naming"), "replace emoji");
    let alternatives = json!([[TextEdit::new(range, "name".into())]]);
    set_suggestion(&mut diagnostic, "Replace emoji", "MachineApplicable", alternatives);
    let mut state = state(&project, false);
    replace_diagnostics(&state, uri.clone(), vec![diagnostic.clone()]);
    params.context.diagnostics.clear();
    assert_eq!(code_actions(&mut state, params.clone()).len(), 1);

    let split_surrogate = Position::new(range.start.line, range.start.character + 1);
    for range in [
        Range::new(split_surrogate, split_surrogate),
        Range::new(range.start, Position::new(range.end.line, u32::MAX)),
        Range::new(range.start, Position::new(u32::MAX, 0)),
        Range::new(range.end, range.start),
    ] {
        params.range = range;
        assert!(code_actions(&mut state, params.clone()).is_empty(), "{range:?}");
    }

    diagnostic.range.start = split_surrogate;
    replace_diagnostics(&state, uri, vec![diagnostic]);
    params.range = range;
    assert!(code_actions(&mut state, params).is_empty());
}

#[test]
fn rejects_stale_disk_and_open_document_fingerprints() {
    let disk = TestProject::from_fixture(&format!("//- /Test.sol\n{BAD_NAME}"));
    let (_, _, _, disk_params) = native_request(&disk);
    disk.write_file("/Test.sol", "contract Test { uint256 changed; }");
    assert!(authorized_code_actions(&mut state(&disk, false), disk_params).is_empty());

    let open = TestProject::from_fixture(&format!("//- /Test.sol open\n{BAD_NAME}"));
    let (uri, _, _, open_params) = native_request(&open);
    let mut open_state = state(&open, false);
    open_state.vfs.write().set_file_contents_with_version(
        crate::proto::vfs_path(&uri).unwrap(),
        Some(crop::Rope::from("contract Test { uint256 changed; }")),
        Some(1),
    );
    assert!(authorized_code_actions(&mut open_state, open_params).is_empty());
}

#[test]
fn canonicalizes_equivalent_file_uris_before_validating_diagnostic_data() {
    let project = TestProject::from_fixture(&format!("//- /Test.sol\n{BAD_NAME}"));
    let (uri, _, diagnostic, mut params) = native_request(&project);
    let encoded = Url::parse(&uri.as_str().replacen("Test.sol", "%54est.sol", 1)).unwrap();
    assert_ne!(uri, encoded);
    assert_eq!(uri.to_file_path(), encoded.to_file_path());
    params.text_document.uri = encoded;
    let mut state = state(&project, false);
    replace_diagnostics(&state, uri, vec![diagnostic]);

    assert_data_eq!(
        actions_output(&code_actions(&mut state, params)),
        str![[r#"
convert the name to mixedCase preferred=Some(true)
  Test.sol 0:24-0:32 "badName"

"#]]
    );
}

#[test]
fn selects_current_server_diagnostics_for_the_client_context() {
    let project = TestProject::from_fixture(&format!("//- /Test.sol\n{BAD_NAME}"));
    let (uri, _, diagnostic, params) = native_request(&project);
    let mut state = state(&project, false);
    let fix = str![[r#"
convert the name to mixedCase preferred=Some(true)
  Test.sol 0:24-0:32 "badName"

"#]];
    let mut check = |server: Vec<Diagnostic>, client: Vec<Diagnostic>, expected: &[&Diagnostic]| {
        replace_diagnostics(&state, uri.clone(), server);
        let mut params = params.clone();
        params.context.diagnostics = client;
        let response = code_actions(&mut state, params);
        assert_eq!(
            action_diagnostics(&response),
            expected.iter().map(|d| (*d).clone()).collect::<Vec<_>>()
        );
        actions_output(&response)
    };

    // Diagnostics outside the current server report are ignored.
    assert!(check(Vec::new(), vec![diagnostic.clone()], &[]).is_empty());
    // An empty client context uses every current server diagnostic.
    assert_data_eq!(check(vec![diagnostic.clone()], Vec::new(), &[&diagnostic]), fix.clone());

    // Stale client presentation and changed or missing data use the server-owned diagnostic.
    let mut stale = diagnostic.clone();
    stale.message = "stale client message".into();
    stale.severity = Some(DiagnosticSeverity::ERROR);
    let mut changed = diagnostic.clone();
    changed.data.as_mut().unwrap()["suggestions"][0]["alternatives"][0][0]["newText"] =
        json!("clientControlled");
    let mut missing = diagnostic.clone();
    missing.data = None;
    for client in [stale, changed, missing] {
        assert_data_eq!(check(vec![diagnostic.clone()], vec![client], &[&diagnostic]), fix.clone());
    }

    // A client context that omits server diagnostics still receives all of their fixes.
    let mut omitted = diagnostic.clone();
    omitted.code = Some(NumberOrString::String("different-lint".into()));
    omitted.message = "a different server diagnostic".into();
    set_suggestion(
        &mut omitted,
        "apply different fix",
        "MachineApplicable",
        json!([[TextEdit::new(diagnostic.range, "differentName".into())]]),
    );
    assert_data_eq!(
        check(
            vec![diagnostic.clone(), omitted.clone()],
            vec![diagnostic.clone()],
            &[&diagnostic, &omitted]
        ),
        str![[r#"
convert the name to mixedCase preferred=Some(true)
  Test.sol 0:24-0:32 "badName"
apply different fix preferred=Some(true)
  Test.sol 0:24-0:32 "differentName"

"#]]
    );

    // Presentation that appends related information still selects the matching server fix.
    let mut first = diagnostic.clone();
    first.related_information = Some(vec![lsp_types::DiagnosticRelatedInformation {
        location: lsp_types::Location::new(uri.clone(), first.range),
        message: "related declaration".into(),
    }]);
    let mut second = first.clone();
    set_suggestion(
        &mut second,
        "apply alternate fix",
        "MachineApplicable",
        json!([[TextEdit::new(diagnostic.range, "alternateName".into())]]),
    );
    let mut published = first.clone();
    published.message.push_str("\nrelated declaration");
    published.related_information = None;
    assert_data_eq!(check(vec![first, second], vec![published.clone()], &[&published]), fix);
}

#[test]
fn returns_no_literal_action_when_the_client_did_not_advertise_support() {
    let project = TestProject::from_fixture(&format!("//- /Test.sol\n{BAD_NAME}"));
    let (uri, _, diagnostic, params) = native_request(&project);
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    *state.vfs.write() = project.vfs();
    replace_diagnostics(&state, uri, vec![diagnostic]);

    assert!(code_actions(&mut state, params).is_empty());
}

#[test]
fn omits_optional_fields_but_keeps_server_owned_fix_data() {
    let project = TestProject::from_fixture(&format!("//- /Test.sol\n{BAD_NAME}"));
    let (uri, _, diagnostic, mut params) = native_request(&project);
    params.context.diagnostics[0].data = None;
    let mut state = state_with_capabilities(&project, false, false, true, false, false);
    replace_diagnostics(&state, uri.clone(), vec![diagnostic]);

    let diagnostics = full_pull_report(&state, uri);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].data.is_none());

    let response = code_actions(&mut state, params);
    assert!(action_diagnostics(&response)[0].data.is_none());
    assert_data_eq!(
        actions_output(&response),
        str![[r#"
convert the name to mixedCase preferred=None
  Test.sol 0:24-0:32 "badName"

"#]]
    );
}

#[test]
fn pull_only_diagnostic_data_support_preserves_quick_fixes() {
    let project = TestProject::from_fixture(&format!("//- /Test.sol\n{BAD_NAME}"));
    let (uri, _, diagnostic, mut params) = native_request(&project);
    let mut state = state_with_capabilities(&project, false, true, true, false, true);
    replace_diagnostics(&state, uri.clone(), vec![diagnostic]);

    params.context.diagnostics = full_pull_report(&state, uri);
    assert!(params.context.diagnostics[0].data.is_some());

    let response = code_actions(&mut state, params);
    assert!(action_diagnostics(&response)[0].data.is_some());
    assert_data_eq!(
        actions_output(&response),
        str![[r#"
convert the name to mixedCase preferred=Some(true)
  Test.sol 0:24-0:32 "badName"

"#]]
    );
}

/// Checks each fallback case with its diagnostic duplicated, which must not duplicate fixes.
fn check_fallbacks(cases: &[FallbackCase<'_>], expected: impl IntoData) {
    let mut output = String::new();
    for &(source, target, origin, code, message) in cases {
        let project = TestProject::new();
        project.write_file("/Test.sol", source);
        let start = source.find(target).unwrap();
        let range = lsp_range(source, start, start + target.len());
        let (_, diagnostic, mut params) = fallback_request(&project, range, origin, code, message);
        params.context.diagnostics.push(diagnostic.clone());
        let response = authorized_code_actions(&mut state(&project, false), params);
        assert!(action_diagnostics(&response).iter().all(|action| *action == diagnostic));
        write!(output, "{}\n{}", format!("== {target}").trim_end(), actions_output(&response))
            .unwrap();
    }
    assert_data_eq!(output, expected.into_data().raw());
}

fn native_request(project: &TestProject) -> (Url, TextEdit, Diagnostic, CodeActionParams) {
    let contents = project.read_file("/Test.sol");
    let start = contents.find("bad_name").unwrap();
    let edit = TextEdit::new(lsp_range(&contents, start, start + 8), "badName".into());
    let (uri, mut diagnostic, mut params) = fallback_request(
        project,
        edit.range,
        "solar",
        Some("mixed-case-variable"),
        "mutable variables should use mixedCase",
    );
    let alternatives = json!([[edit]]);
    set_suggestion(
        &mut diagnostic,
        "convert the name to mixedCase",
        "MachineApplicable",
        alternatives,
    );
    params.context.diagnostics = vec![diagnostic.clone()];
    (uri, edit, diagnostic, params)
}

fn fallback_request(
    project: &TestProject,
    range: Range,
    source: &str,
    code: Option<&str>,
    message: &str,
) -> (Url, Diagnostic, CodeActionParams) {
    let contents = project.read_file("/Test.sol");
    let uri = project.uri("/Test.sol");
    let diagnostic = Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::WARNING),
        code: code.map(|code| NumberOrString::String(code.into())),
        source: Some(source.into()),
        data: Some(json!({
            "version": 1,
            "uri": uri,
            "sourceFingerprint": source_fingerprint(&contents),
            "suggestions": []
        })),
        ..Diagnostic::new_simple(range, message.into())
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

fn set_suggestion(
    diagnostic: &mut Diagnostic,
    title: &str,
    applicability: &str,
    alternatives: serde_json::Value,
) {
    diagnostic.data.as_mut().unwrap()["suggestions"] =
        json!([{ "title": title, "applicability": applicability, "alternatives": alternatives }]);
}

fn native_data(uri: Url, contents: &str, title: &str, edit: TextEdit) -> serde_json::Value {
    let suggestion =
        DiagnosticSuggestion::new(title.into(), Applicability::MachineApplicable, vec![vec![edit]]);
    DiagnosticData::new(uri, contents, vec![suggestion]).to_value()
}

fn lsp_range(contents: &str, start: usize, end: usize) -> Range {
    let contents = crop::Rope::from(contents);
    Range::new(
        crate::proto::position_at_byte(&contents, start).unwrap(),
        crate::proto::position_at_byte(&contents, end).unwrap(),
    )
}

fn state(project: &TestProject, document_changes: bool) -> GlobalState {
    state_with_capabilities(project, document_changes, true, false, true, true)
}

fn state_with_capabilities(
    project: &TestProject,
    document_changes: bool,
    is_preferred: bool,
    pull_delivery: bool,
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
    text_document.diagnostic = pull_delivery.then(DiagnosticClientCapabilities::default);
    if pull_delivery {
        initialize.capabilities.workspace.get_or_insert_default().diagnostic =
            Some(DiagnosticWorkspaceClientCapabilities { refresh_support: Some(true) });
    }
    if document_changes {
        initialize.capabilities.workspace.get_or_insert_default().workspace_edit =
            Some(WorkspaceEditClientCapabilities {
                document_changes: Some(true),
                ..Default::default()
            });
    }
    let config = negotiate_capabilities_with_pull_diagnostic_data(
        initialize,
        pull_diagnostic_data,
        &LaunchConfig::default(),
    )
    .1;
    state.config = Arc::new(config);
    *state.vfs.write() = project.vfs();
    state
}

fn full_pull_report(state: &GlobalState, uri: Url) -> Vec<Diagnostic> {
    let report = block_on(state.pull_diagnostic_report(uri, None)).unwrap();
    let crate::diagnostics::PullReport::Full { diagnostics, .. } = report else {
        panic!("expected a full diagnostic report");
    };
    diagnostics
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
    code_actions(state, params)
}

fn code_actions(state: &mut GlobalState, params: CodeActionParams) -> Vec<CodeActionOrCommand> {
    block_on(crate::handlers::code_actions(state, params)).unwrap().unwrap()
}

fn replace_diagnostics(state: &GlobalState, uri: Url, diagnostics: Vec<Diagnostic>) {
    let mut diagnostic_map = crate::diagnostics::DiagnosticMap::default();
    diagnostic_map.insert(uri, diagnostics);
    state.replace_diagnostics_for_test(diagnostic_map);
}

/// Returns the single diagnostic that each quick fix resolves.
fn action_diagnostics(response: &[CodeActionOrCommand]) -> Vec<Diagnostic> {
    response
        .iter()
        .map(|action| {
            let CodeActionOrCommand::CodeAction(action) = action else {
                panic!("expected a literal action, got {action:?}")
            };
            let [diagnostic] = action.diagnostics.as_deref().unwrap() else {
                panic!("expected one diagnostic, got {action:?}")
            };
            diagnostic.clone()
        })
        .collect()
}

fn actions_output(response: &[CodeActionOrCommand]) -> String {
    let mut output = String::new();
    for action in response {
        let CodeActionOrCommand::CodeAction(action) = action else {
            panic!("expected a literal action, got {action:?}")
        };
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        writeln!(output, "{} preferred={:?}", action.title, action.is_preferred).unwrap();
        let edit = action.edit.as_ref().unwrap();
        assert!(edit.change_annotations.is_none());
        let documents = match (&edit.changes, &edit.document_changes) {
            (Some(changes), None) => {
                changes.iter().map(|(uri, edits)| (uri, String::new(), edits.clone())).collect()
            }
            (None, Some(DocumentChanges::Edits(documents))) => documents
                .iter()
                .map(|document| {
                    let edits = document.edits.iter().map(|edit| match edit {
                        OneOf::Left(edit) => edit.clone(),
                        OneOf::Right(edit) => panic!("unexpected annotated edit {edit:?}"),
                    });
                    let version = format!("@{:?}", document.text_document.version);
                    (&document.text_document.uri, version, edits.collect())
                })
                .collect::<Vec<_>>(),
            _ => panic!("expected one form of workspace edit, got {edit:?}"),
        };
        for (uri, version, edits) in documents {
            let name = uri.path().rsplit('/').next().unwrap();
            for TextEdit { range, new_text } in edits {
                let Range { start, end } = range;
                writeln!(
                    output,
                    "  {name}{version} {}:{}-{}:{} {new_text:?}",
                    start.line, start.character, end.line, end.character,
                )
                .unwrap();
            }
        }
    }
    output
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
}
