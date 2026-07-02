use super::super::{AnalysisBatch, AnalysisResult, analyze};
use crate::{symbols::SymbolTables, test_support::TestProject};
use lsp_types::{DocumentSymbol, GotoDefinitionResponse, Location, OneOf, SymbolKind, Url};
use snapbox::{IntoData, assert_data_eq};
use solar_config::CompileOpts;
use solar_interface::data_structures::map::FxHashSet;
use std::{collections::HashMap, fmt::Write, path::Path};

#[test]
fn builds_declaration_symbol_table() {
    check_declarations(
        r#"
        //- /Symbols.sol
        uint256 constant TOP = 1;
        contract C {
            uint256 public x;
            uint256 public constant K = 1;
            struct S { uint256 field; }
            struct GetterValue {
                uint256 visible;
                uint256 other;
                mapping(uint256 => uint256) hidden;
            }
            mapping(uint256 key => uint256 value) public getterMap;
            mapping(uint256 key => GetterValue value) public getterValues;
            constructor() {}
            fallback() external {}
            receive() external payable {}
            function f(uint256 y) public returns (uint256 z) {
                uint256 local = x + y;
                return local;
            }
        }
        enum E { A }
        "#,
        "/Symbols.sol",
        snapbox::str![[r#"
TOP | constant | - | 0:17-0:20
C | class | - | 1:9-1:10
x | property | C | 2:19-2:20
K | constant | C | 3:28-3:29
S | struct | C | 4:11-4:12
field | property | S | 4:23-4:28
GetterValue | struct | C | 5:11-5:22
visible | property | GetterValue | 6:16-6:23
other | property | GetterValue | 7:16-7:21
hidden | property | GetterValue | 8:36-8:42
getterMap | property | C | 10:49-10:58
getterValues | property | C | 11:53-11:65
constructor | constructor | C | 12:4-12:15
fallback | function | C | 13:4-13:12
receive | function | C | 14:4-14:11
f | method | C | 15:13-15:14
y | variable | f | 15:23-15:24
z | variable | f | 15:50-15:51
local | variable | f | 16:16-16:21
E | enum | - | 20:5-20:6
A | enum member | E | 20:9-20:10
"#]],
    );
}

#[test]
fn builds_lsp_symbol_responses() {
    let fixture = r#"
        //- /Symbols.sol
        interface I {
            function iface(uint256 value) external;
        }
        library L {
            event Logged(uint256 value);
            function helper(uint256 value) internal pure returns (uint256 result) {
                return value;
            }
        }
        contract C {
            enum E { A, B }
            struct S { uint256 field; }
            uint256 public x;
            constructor() {}
            function f(uint256 y) public returns (uint256 z) {
                uint256 local = y;
                return local;
            }
        }
        "#;

    check_document_symbols(
        fixture,
        "/Symbols.sol",
        snapbox::str![[r#"
I | interface | 0:10-0:11
  iface | method | 1:13-1:18
    value | variable | 1:27-1:32
L | module | 3:8-3:9
  Logged | event | 4:10-4:16
    value | variable | 4:25-4:30
  helper | method | 5:13-5:19
    value | variable | 5:28-5:33
    result | variable | 5:66-5:72
C | class | 9:9-9:10
  E | enum | 10:9-10:10
    A | enum member | 10:13-10:14
    B | enum member | 10:16-10:17
  S | struct | 11:11-11:12
    field | property | 11:23-11:28
  x | property | 12:19-12:20
  constructor | constructor | 13:4-13:15
  f | method | 14:13-14:14
    y | variable | 14:23-14:24
    z | variable | 14:50-14:51
    local | variable | 15:16-15:21
"#]],
    );

    check_workspace_symbols(
        fixture,
        "helper",
        snapbox::str![[r#"
helper | method | L | /Symbols.sol:5:4-7:5
"#]],
    );

    check_workspace_symbols(
        fixture,
        "",
        snapbox::str![[r#"
I | interface | - | /Symbols.sol:0:0-2:1
iface | method | I | /Symbols.sol:1:4-1:43
value | variable | iface | /Symbols.sol:1:19-1:32
L | module | - | /Symbols.sol:3:0-8:1
Logged | event | L | /Symbols.sol:4:4-4:32
value | variable | Logged | /Symbols.sol:4:17-4:30
helper | method | L | /Symbols.sol:5:4-7:5
value | variable | helper | /Symbols.sol:5:20-5:33
result | variable | helper | /Symbols.sol:5:58-5:72
C | class | - | /Symbols.sol:9:0-18:1
E | enum | C | /Symbols.sol:10:4-10:19
A | enum member | E | /Symbols.sol:10:13-10:14
B | enum member | E | /Symbols.sol:10:16-10:17
S | struct | C | /Symbols.sol:11:4-11:31
field | property | S | /Symbols.sol:11:15-11:28
x | property | C | /Symbols.sol:12:4-12:21
constructor | constructor | C | /Symbols.sol:13:4-13:20
f | method | C | /Symbols.sol:14:4-17:5
y | variable | f | /Symbols.sol:14:15-14:24
z | variable | f | /Symbols.sol:14:42-14:51
local | variable | f | /Symbols.sol:15:8-15:25
"#]],
    );
}

#[test]
fn builds_lsp_navigation_and_reference_indexes() {
    check_locations(
        r#"
        //- /Symbols.sol
        contract C {
            uint256 stateValue;

            function target(uint256 input) public returns (uint256 output) {
                uint256 localValue = input + stateValue;
                output = localValue;
            }

            function caller() public {
                uint256 callerLocal = $1target(stateValue);
            }
        }
        "#,
        Request::Definition,
        snapbox::str![[r#"
/Symbols.sol:2:13-2:19
"#]],
    );

    check_locations(
        r#"
        //- /Symbols.sol
        contract C {
            uint256 stateValue;

            function $1target(uint256 input) public returns (uint256 output) {
                uint256 localValue = input + stateValue;
                output = localValue;
            }

            function caller() public {
                uint256 callerLocal = target(stateValue);
            }
        }
        "#,
        Request::References,
        snapbox::str![[r#"
/Symbols.sol:2:13-2:19
/Symbols.sol:7:30-7:36
"#]],
    );

    check_locations(
        r#"
        //- /Symbols.sol
        contract C {
            uint256 $1stateValue;

            function target(uint256 input) public returns (uint256 output) {
                uint256 localValue = input + stateValue;
                output = localValue;
            }

            function caller() public {
                uint256 callerLocal = target(stateValue);
            }
        }
        "#,
        Request::References,
        snapbox::str![[r#"
/Symbols.sol:1:12-1:22
/Symbols.sol:3:37-3:47
/Symbols.sol:7:37-7:47
"#]],
    );
}

#[test]
fn indexes_member_references() {
    check_locations(
        r#"
        //- /Members.sol
        contract C {
            enum Choice { A, B }
            struct Data { uint256 field; }

            function read(Data memory data) public pure returns (uint256) {
                Choice choice = Choice.A;
                return data.$1field;
            }
        }
        "#,
        Request::Definition,
        snapbox::str![[r#"
/Members.sol:2:26-2:31
"#]],
    );

    check_locations(
        r#"
        //- /Members.sol
        contract C {
            enum Choice { A, B }
            struct Data { uint256 field; }

            function read(Data memory data) public pure returns (uint256) {
                Choice choice = Choice.$1A;
                return data.field;
            }
        }
        "#,
        Request::Definition,
        snapbox::str![[r#"
/Members.sol:1:18-1:19
"#]],
    );

    check_locations(
        r#"
        //- /Members.sol
        contract C {
            enum Choice { A, B }
            struct Data { uint256 $1field; }

            function read(Data memory data) public pure returns (uint256) {
                Choice choice = Choice.A;
                return data.field;
            }
        }
        "#,
        Request::References,
        snapbox::str![[r#"
/Members.sol:2:26-2:31
/Members.sol:5:20-5:25
"#]],
    );
}

#[test]
fn skips_generated_getter_references() {
    check_locations(
        r#"
        //- /Getter.sol
        contract C {
            uint256 public $1x;

            function read() external view returns (uint256) {
                return x;
            }
        }
        "#,
        Request::References,
        snapbox::str![[r#"
/Getter.sol:1:19-1:20
/Getter.sol:3:15-3:16
"#]],
    );
}

#[test]
fn resolves_overloaded_call_references() {
    check_locations(
        r#"
        //- /Overload.sol
        contract C {
            function f(uint256) public {}
            function f(string memory) public {}
            function g() public {
                $1f(uint256(1));
            }
        }
        "#,
        Request::Definition,
        snapbox::str![[r#"
/Overload.sol:1:13-1:14
"#]],
    );

    check_locations(
        r#"
        //- /Overload.sol
        contract C {
            function $1f(uint256) public {}
            function f(string memory) public {}
            function g() public {
                f(uint256(1));
            }
        }
        "#,
        Request::References,
        snapbox::str![[r#"
/Overload.sol:1:13-1:14
/Overload.sol:4:8-4:9
"#]],
    );

    check_locations(
        r#"
        //- /Overload.sol
        contract C {
            function f(uint256) public {}
            function $1f(string memory) public {}
            function g() public {
                f(uint256(1));
            }
        }
        "#,
        Request::References,
        snapbox::str![[r#"
/Overload.sol:2:13-2:14
"#]],
    );
}

#[test]
fn indexes_using_directive_references() {
    check_locations(
        r#"
        //- /Using.sol
        library L {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        using $1L for uint256;

        contract C {
            function f(uint256 value) public pure returns (uint256) {
                return value.inc();
            }
        }
        "#,
        Request::Definition,
        snapbox::str![[r#"
/Using.sol:0:8-0:9
"#]],
    );

    check_locations(
        r#"
        //- /Using.sol
        library $1L {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        using L for uint256;

        contract C {
            function f(uint256 value) public pure returns (uint256) {
                return value.inc();
            }
        }
        "#,
        Request::References,
        snapbox::str![[r#"
/Using.sol:0:8-0:9
/Using.sol:5:6-5:7
"#]],
    );

    check_locations(
        r#"
        //- /Using.sol
        library L {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        using L for uint256;

        contract C {
            function f(uint256 value) public pure returns (uint256) {
                return value.$1inc();
            }
        }
        "#,
        Request::Definition,
        snapbox::str![[r#"
/Using.sol:1:13-1:16
"#]],
    );
}

#[test]
fn distinguishes_function_declarations_from_definitions() {
    check_locations(
        r#"
        //- /Navigation.sol
        interface I {
            function $1f() external returns (uint256);
        }
        "#,
        Request::Declaration,
        snapbox::str![[r#"
/Navigation.sol:1:13-1:14
"#]],
    );

    check_locations(
        r#"
        //- /Navigation.sol
        interface I {
            function $1f() external returns (uint256);
        }
        "#,
        Request::Definition,
        snapbox::str![[r#"
-
"#]],
    );
}

#[derive(Clone, Copy)]
enum Request {
    Declaration,
    Definition,
    References,
}

fn check_declarations(fixture: &str, path: &str, expected: impl IntoData) {
    let project = TestProject::from_fixture(fixture);
    let result = analyze_single_file(&project, path);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);

    let uri = Url::from_file_path(project.path(path)).unwrap();
    assert_data_eq!(format_declarations(&result.symbol_tables, &uri), expected);
}

fn check_document_symbols(fixture: &str, path: &str, expected: impl IntoData) {
    let project = TestProject::from_fixture(fixture);
    let result = analyze_single_file(&project, path);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);

    let uri = Url::from_file_path(project.path(path)).unwrap();
    assert_data_eq!(
        format_document_symbols(&result.symbol_tables.document_symbols(&uri)),
        expected
    );
}

fn check_workspace_symbols(fixture: &str, query: &str, expected: impl IntoData) {
    let project = TestProject::from_fixture(fixture);
    let result = analyze_single_file(&project, "/Symbols.sol");
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);

    assert_data_eq!(format_workspace_symbols(&project, &result.symbol_tables, query), expected);
}

fn check_locations(fixture: &str, request: Request, expected: impl IntoData) {
    let fixture = TestProject::from_fixture_with_cursor(fixture);
    let result = analyze_fixture(fixture.files);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);

    let locations = match request {
        Request::Declaration => result
            .symbol_tables
            .goto_declaration(&fixture.cursor.uri, fixture.cursor.position)
            .and_then(response_locations),
        Request::Definition => result
            .symbol_tables
            .goto_definition(&fixture.cursor.uri, fixture.cursor.position)
            .and_then(response_locations),
        Request::References => {
            result.symbol_tables.references(&fixture.cursor.uri, fixture.cursor.position, true)
        }
    };

    assert_data_eq!(format_locations(&fixture.project, locations.as_deref()), expected);
}

fn analyze_single_file(project: &TestProject, path: &str) -> AnalysisResult {
    analyze_fixture([(project.path(path), project.read_file(path))])
}

fn analyze_fixture(
    files: impl IntoIterator<Item = (std::path::PathBuf, String)>,
) -> AnalysisResult {
    analyze(AnalysisBatch {
        opts: CompileOpts::default(),
        files: files.into_iter().collect(),
        seen_paths: FxHashSet::default(),
    })
}

fn format_declarations(tables: &SymbolTables, uri: &Url) -> String {
    let declarations = tables.file_declarations(uri).collect::<Vec<_>>();
    assert_eq!(declarations.len(), tables.declarations().len());
    let names_by_id = declarations
        .iter()
        .map(|declaration| (declaration.id, declaration.name.as_str()))
        .collect::<HashMap<_, _>>();
    let mut output = String::new();
    for declaration in declarations {
        let parent =
            declaration.parent.and_then(|parent| names_by_id.get(&parent).copied()).unwrap_or("-");
        writeln!(
            &mut output,
            "{} | {} | {parent} | {}",
            declaration.name,
            symbol_kind(declaration.kind),
            format_range(declaration.name_range),
        )
        .unwrap();
    }
    finish_output(output)
}

fn format_document_symbols(symbols: &[DocumentSymbol]) -> String {
    let mut output = String::new();
    for symbol in symbols {
        format_document_symbol(&mut output, symbol, 0);
    }
    finish_output(output)
}

fn format_document_symbol(output: &mut String, symbol: &DocumentSymbol, depth: usize) {
    let indent = "  ".repeat(depth);
    writeln!(
        output,
        "{indent}{} | {} | {}",
        symbol.name,
        symbol_kind(symbol.kind),
        format_range(symbol.selection_range),
    )
    .unwrap();
    for child in symbol.children.as_deref().unwrap_or_default() {
        format_document_symbol(output, child, depth + 1);
    }
}

fn format_workspace_symbols(project: &TestProject, tables: &SymbolTables, query: &str) -> String {
    let mut output = String::new();
    for symbol in tables.workspace_symbols(query) {
        let location = match symbol.location {
            OneOf::Left(location) => format_location(project, &location),
            OneOf::Right(location) => location.uri.to_string(),
        };
        writeln!(
            &mut output,
            "{} | {} | {} | {location}",
            symbol.name,
            symbol_kind(symbol.kind),
            symbol.container_name.as_deref().unwrap_or("-"),
        )
        .unwrap();
    }
    finish_output(output)
}

fn format_locations(project: &TestProject, locations: Option<&[Location]>) -> String {
    let Some(locations) = locations else {
        return "-".to_string();
    };
    if locations.is_empty() {
        return "-".to_string();
    }

    let mut output = String::new();
    for location in locations {
        writeln!(&mut output, "{}", format_location(project, location)).unwrap();
    }
    finish_output(output)
}

fn response_locations(response: GotoDefinitionResponse) -> Option<Vec<Location>> {
    match response {
        GotoDefinitionResponse::Array(locations) => Some(locations),
        GotoDefinitionResponse::Scalar(location) => Some(vec![location]),
        GotoDefinitionResponse::Link(_) => None,
    }
}

fn format_location(project: &TestProject, location: &Location) -> String {
    format!("{}:{}", format_uri(project, &location.uri), format_range(location.range))
}

pub(super) fn format_uri(project: &TestProject, uri: &Url) -> String {
    let Ok(path) = uri.to_file_path() else {
        return uri.to_string();
    };
    display_path(project.root(), &path)
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .and_then(|path| path.to_str())
        .map(|path| format!("/{path}"))
        .unwrap_or_else(|| path.display().to_string())
}

fn format_range(range: lsp_types::Range) -> String {
    format!(
        "{}:{}-{}:{}",
        range.start.line, range.start.character, range.end.line, range.end.character
    )
}

fn finish_output(mut output: String) -> String {
    if output.ends_with('\n') {
        output.pop();
    }
    output
}

fn symbol_kind(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::FILE => "file",
        SymbolKind::MODULE => "module",
        SymbolKind::NAMESPACE => "namespace",
        SymbolKind::PACKAGE => "package",
        SymbolKind::CLASS => "class",
        SymbolKind::METHOD => "method",
        SymbolKind::PROPERTY => "property",
        SymbolKind::FIELD => "field",
        SymbolKind::CONSTRUCTOR => "constructor",
        SymbolKind::ENUM => "enum",
        SymbolKind::INTERFACE => "interface",
        SymbolKind::FUNCTION => "function",
        SymbolKind::VARIABLE => "variable",
        SymbolKind::CONSTANT => "constant",
        SymbolKind::STRING => "string",
        SymbolKind::NUMBER => "number",
        SymbolKind::BOOLEAN => "boolean",
        SymbolKind::ARRAY => "array",
        SymbolKind::OBJECT => "object",
        SymbolKind::KEY => "key",
        SymbolKind::NULL => "null",
        SymbolKind::ENUM_MEMBER => "enum member",
        SymbolKind::STRUCT => "struct",
        SymbolKind::EVENT => "event",
        SymbolKind::OPERATOR => "operator",
        SymbolKind::TYPE_PARAMETER => "type parameter",
        _ => "unknown",
    }
}
