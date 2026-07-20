use super::{AnalysisBatch, GlobalState, SymbolTables, analyze, support::RequestFixture};
use crate::test_support::TestProject;
use async_lsp::ClientSocket;
use lsp_types::{
    HoverContents, HoverParams, Position, TextDocumentIdentifier, TextDocumentPositionParams, Url,
    WorkDoneProgressParams,
};
use snapbox::str;
use solar_config::CompileOpts;
use solar_interface::data_structures::map::FxHashSet;
use std::{
    sync::atomic::Ordering,
    task::{Context, Poll, Waker},
};

#[test]
fn shows_function_signature_at_a_reference() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hover.sol open
        contract C {
            function $1add(uint256 value) public pure {}

            function use() public pure {
                $2add(1);
            }
        }
        "#,
        "/Hover.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
1:13-1:16
```solidity
function add(uint256 value) public pure
```

"#]],
    );
    fixture.check_hover(
        "$2",
        str![[r#"
3:8-3:11
```solidity
function add(uint256 value) public pure
```

"#]],
    );
}

#[test]
fn includes_resolved_natspec_documentation() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hover.sol open
        contract C {
            /// @notice Updates the stored value.
            /// @dev The caller is responsible for choosing the value.
            /// @param value The next value.
            /// @return result The normalized value.
            function set(uint256 $2value) public pure returns (uint256 $3result) {
                result = value;
            }

            function use() public pure {
                $1set(1);
            }
        }
        "#,
        "/Hover.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
9:8-9:11
```solidity
function set(uint256 value) public pure returns (uint256 result)
```

Updates the stored value.

**@dev**

The caller is responsible for choosing the value.

**@param**

- `value`: The next value.

**@return**

- `result`: The normalized value.

"#]],
    );
    fixture.check_hover(
        "$2",
        str![[r#"
5:25-5:30
```solidity
uint256 value
```

**@param**

- `value`: The next value.

"#]],
    );
    fixture.check_hover(
        "$3",
        str![[r#"
5:61-5:67
```solidity
uint256 result
```

**@return**

- `result`: The normalized value.

"#]],
    );
}

#[test]
fn maps_mixed_named_and_unnamed_return_docs_by_position() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hover.sol open
        contract C {
            /// @return The first return value.
            /// @return result The named return value.
            function read() public pure returns (uint256, uint256 result) {
                return (1, 2);
            }

            function use() public pure {
                $1read();
            }
        }
        "#,
        "/Hover.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
7:8-7:12
```solidity
function read() public pure returns (uint256, uint256 result)
```

**@return**

- The first return value.

- `result`: The named return value.

"#]],
    );
}

#[test]
fn preserves_unnamed_return_docs_after_an_invalid_named_tag() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Hover.sol open
        contract C {
            /// @return wrong This tag is invalid.
            /// @return The unnamed return value.
            function read() public pure returns (uint256 first, uint256) {
                return (1, 2);
            }

            function use() public pure {
                $1read();
            }
        }
        "#,
        "/Hover.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
7:8-7:12
```solidity
function read() public pure returns (uint256 first, uint256)
```

**@return**

- The unnamed return value.

"#]],
    );
}

#[test]
fn includes_local_docs_for_multi_return_public_getters() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hover.sol open
        contract C {
            struct Record {
                uint256 value;
                address owner;
            }

            /// @return value The stored value.
            /// @return owner The record owner.
            Record public $1record;
        }
        "#,
        "/Hover.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
7:18-7:24
```solidity
Record public record
```

**@return**

- `value`: The stored value.

- `owner`: The record owner.

"#]],
    );
}

#[test]
fn maps_inherited_docs_to_current_public_getter_returns() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hover.sol open
        interface Base {
            /// @return first The first base value.
            /// @return second The second base value.
            function record() external view returns (uint256 first, address second);
        }

        contract Child is Base {
            struct Record {
                uint256 value;
                address owner;
            }

            /// @inheritdoc Base
            Record public override $1record;
        }
        "#,
        "/Hover.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
11:27-11:33
```solidity
Record public record
```

**@return**

- `value`: The first base value.

- `owner`: The second base value.

"#]],
    );
}

#[test]
fn shows_variable_types_and_attributes() {
    let fixture = RequestFixture::new(
        r#"
        //- /Variables.sol open
        type UserId is uint256;
        contract C {
            mapping(address => UserId) private $1ids;
            uint256 public constant $7LIMIT = 10;
            address immutable $8owner;

            function use(UserId[] calldata $3values) external {
                UserId[] memory $5local;
                $2ids[msg.sender] = $4values[0];
                $6local = values;
            }
        }
        "#,
        "/Variables.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
2:39-2:42
```solidity
mapping(address => UserId) private ids
```

"#]],
    );
    fixture.check_hover(
        "$2",
        str![[r#"
7:8-7:11
```solidity
mapping(address => UserId) private ids
```

"#]],
    );
    fixture.check_hover(
        "$3",
        str![[r#"
5:35-5:41
```solidity
UserId[] calldata values
```

"#]],
    );
    fixture.check_hover(
        "$4",
        str![[r#"
7:26-7:32
```solidity
UserId[] calldata values
```

"#]],
    );
    fixture.check_hover(
        "$5",
        str![[r#"
6:24-6:29
```solidity
UserId[] memory local
```

"#]],
    );
    fixture.check_hover(
        "$6",
        str![[r#"
8:8-8:13
```solidity
UserId[] memory local
```

"#]],
    );
    fixture.check_hover(
        "$7",
        str![[r#"
3:28-3:33
```solidity
uint256 public constant LIMIT
```

"#]],
    );
    fixture.check_hover(
        "$8",
        str![[r#"
4:22-4:27
```solidity
address immutable owner
```

"#]],
    );
}

#[test]
fn uses_the_type_checked_overload() {
    let fixture = RequestFixture::new(
        r#"
        //- /Overloads.sol open
        contract C {
            function pick(string memory value) public pure returns (string memory) { return value; }
            function pick(uint256 value) public pure returns (uint256) { return value; }

            function use() public pure returns (uint256) {
                return $1pick(1);
            }
        }
        "#,
        "/Overloads.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
4:15-4:19
```solidity
function pick(uint256 value) public pure returns (uint256)
```

"#]],
    );
}

#[test]
fn returns_no_hover_for_an_ambiguous_overload() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Ambiguous.sol open
        contract C {
            function pick(uint8 value) internal pure returns (uint8) {
                return value;
            }

            function pick(uint256 value) internal pure returns (uint256) {
                return value;
            }

            function call(uint8 value) public pure returns (uint256) {
                return $1pick(value);
            }
        }
        "#,
        "/Ambiguous.sol",
    );

    fixture.check_hover("$1", "<none>\n");
}

#[test]
fn shows_special_functions_and_modifiers() {
    let fixture = RequestFixture::new(
        r#"
        //- /Special.sol open
        contract C {
            modifier $1limited(uint256 amount) { require(amount > 0); _; }
            $2constructor(uint256 count) payable { require(count > 0); }
            $3fallback() external payable {}
            $4receive() external payable {}
        }
        "#,
        "/Special.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
1:13-1:20
```solidity
modifier limited(uint256 amount)
```

"#]],
    );
    fixture.check_hover(
        "$2",
        str![[r#"
2:4-2:15
```solidity
constructor(uint256 count) payable
```

"#]],
    );
    fixture.check_hover(
        "$3",
        str![[r#"
3:4-3:12
```solidity
fallback() external payable
```

"#]],
    );
    fixture.check_hover(
        "$4",
        str![[r#"
4:4-4:11
```solidity
receive() external payable
```

"#]],
    );
}

#[test]
fn resolves_inherited_cross_file_symbols_and_inheritdoc() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        contract Base {
            /// @notice Updates the value.
            /// @param value The next value.
            /// @return result The stored value.
            function update(uint256 value) public pure virtual returns (uint256 result) {
                return value;
            }

            /// @notice Emitted after an update.
            /// @param value The emitted value.
            event Updated(uint256 indexed $6value) anonymous;

            /// @notice The account is forbidden.
            /// @param account The rejected account.
            error Forbidden(address account);
        }
        //- /Use.sol open
        import {Base} from "./Base.sol";
        contract Child is Base {
            modifier onlyReady() { _; }

            /// @inheritdoc Base
            function update(uint256 $4amount) public pure override onlyReady returns (uint256 $5out) {
                out = amount;
            }

            function run(address account) public returns (uint256) {
                emit $1Updated(1);
                if (account == address(0)) {
                    revert $2Forbidden(account);
                }
                return $3update(1);
            }
        }
        "#,
        "/Use.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
8:13-8:20
```solidity
event Updated(uint256 indexed value) anonymous
```

Emitted after an update.

**@param**

- `value`: The emitted value.

"#]],
    );
    fixture.check_hover(
        "$6",
        str![[r#"
9:34-9:39
```solidity
uint256 indexed value
```

**@param**

- `value`: The emitted value.

"#]],
    );
    fixture.check_hover(
        "$2",
        str![[r#"
10:19-10:28
```solidity
error Forbidden(address account)
```

The account is forbidden.

**@param**

- `account`: The rejected account.

"#]],
    );
    fixture.check_hover(
        "$3",
        str![[r#"
12:15-12:21
```solidity
function update(uint256 amount) public pure override onlyReady returns (uint256 out)
```

Updates the value.

**@param**

- `amount`: The next value.

**@return**

- `out`: The stored value.

"#]],
    );
    fixture.check_hover(
        "$4",
        str![[r#"
4:28-4:34
```solidity
uint256 amount
```

**@param**

- `amount`: The next value.

"#]],
    );
    fixture.check_hover(
        "$5",
        str![[r#"
4:84-4:87
```solidity
uint256 out
```

**@return**

- `out`: The stored value.

"#]],
    );
}

#[test]
fn maps_inherited_documentation_by_position_when_names_collide() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        contract Base {
            /// @notice Chooses a value.
            /// @param first The first value.
            /// @param second The second value.
            /// @return firstOut The first result.
            /// @return secondOut The second result.
            function choose(uint256 first, uint256 second)
                public pure virtual returns (uint256 firstOut, uint256 secondOut)
            {
                return (first, second);
            }
        }
        //- /Child.sol open
        import {Base} from "./Base.sol";
        contract Child is Base {
            /// @inheritdoc Base
            function choose(uint256 second, uint256 third)
                public pure override returns (uint256 secondOut, uint256 thirdOut)
            {
                return (second, third);
            }

            function use() public pure {
                $1choose(1, 2);
            }
        }
        "#,
        "/Child.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
9:8-9:14
```solidity
function choose(uint256 second, uint256 third) public pure override returns (uint256 secondOut, uint256 thirdOut)
```

Chooses a value.

**@param**

- `second`: The first value.

- `third`: The second value.

**@return**

- `secondOut`: The first result.

- `thirdOut`: The second result.

"#]],
    );
}

#[test]
fn uses_the_explicit_inheritdoc_source_for_positional_documentation() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hover.sol open
        contract First {
            /// @param right The first contract's left value.
            /// @param left The first contract's right value.
            function choose(uint256 right, uint256 left) public pure virtual {}
        }

        contract Second {
            /// @param left The second contract's left value.
            /// @param right The second contract's right value.
            function choose(uint256 left, uint256 right) public pure virtual {}
        }

        contract Child is First, Second {
            /// @inheritdoc Second
            function choose(uint256 first, uint256 second)
                public pure override(First, Second)
            {}

            function use() public pure {
                $1choose(1, 2);
            }
        }
        "#,
        "/Hover.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
16:8-16:14
```solidity
function choose(uint256 first, uint256 second) public pure override(First, Second)
```

**@param**

- `first`: The second contract's left value.

- `second`: The second contract's right value.

"#]],
    );
}

#[test]
fn ignores_invalid_local_tags_when_resolving_inheritdoc() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Hover.sol open
        contract Base {
            /// @param value The inherited value.
            /// @return result The inherited result.
            function update(uint256 value) public pure virtual returns (uint256 result) {}
        }

        contract Child is Base {
            /// @param missing This tag is invalid.
            /// @return missing This tag is also invalid.
            /// @inheritdoc Base
            function update(uint256 renamed)
                public pure override returns (uint256 renamedResult)
            {}

            function use() public pure {
                $1update(1);
            }
        }
        "#,
        "/Hover.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
13:8-13:14
```solidity
function update(uint256 renamed) public pure override returns (uint256 renamedResult)
```

**@param**

- `renamed`: The inherited value.

**@return**

- `renamedResult`: The inherited result.

"#]],
    );
}

#[test]
fn follows_positional_documentation_through_multiple_inheritdoc_levels() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hover.sol open
        contract Base {
            /// @param value The base value.
            /// @return result The base result.
            function read(uint256 value) public pure virtual returns (uint256 result) {}
        }

        contract Middle is Base {
            /// @inheritdoc Base
            function read(uint256 middleValue)
                public pure virtual override returns (uint256 middleResult)
            {}
        }

        contract Leaf is Middle {
            /// @inheritdoc Middle
            function read(uint256 leafValue)
                public pure override returns (uint256 leafResult)
            {}

            function use() public pure {
                $1read(1);
            }
        }
        "#,
        "/Hover.sol",
    );

    fixture.check_hover(
        "$1",
        str![[r#"
17:8-17:12
```solidity
function read(uint256 leafValue) public pure override returns (uint256 leafResult)
```

**@param**

- `leafValue`: The base value.

**@return**

- `leafResult`: The base result.

"#]],
    );
}

#[test]
fn returns_no_hover_for_unsupported_or_non_symbol_positions() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Unsupported.sol open
        contract $1C {
            struct $2Data {}
            enum $3Kind { A }

            function use() public returns (uint256) {
                $7uint256 value = $4missing;
                return $8 1;
            }

            function empty() public { $5
            }
        }
        "#,
        "/Unsupported.sol",
    );

    for marker in ["$1", "$2", "$3", "$4", "$5", "$7", "$8"] {
        fixture.check_hover(marker, "<none>\n");
    }
}

#[test]
fn preserves_hover_payloads_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /First.sol open
        contract First {
            uint256 $1one;
        }
        //- /Second.sol open
        contract Second {
            function $2two(address account) external pure returns (address) {
                return account;
            }
        }
        "#,
        &["/First.sol", "/Second.sol"],
    );

    fixture.check_hover(
        "$1",
        str![[r#"
1:12-1:15
```solidity
uint256 one
```

"#]],
    );
    fixture.check_hover(
        "$2",
        str![[r#"
1:13-1:16
```solidity
function two(address account) external pure returns (address)
```

"#]],
    );
}

#[test]
fn waits_for_current_analysis_before_returning_hover() {
    let project = TestProject::from_fixture(
        r#"
        //- /Fresh.sol
        contract C {
            uint256 oldValue;
        }
        "#,
    );
    let path = project.path("/Fresh.sol");
    let old_tables = analyze(AnalysisBatch {
        opts: CompileOpts::default(),
        files: vec![(path.clone(), project.read_file("/Fresh.sol"))],
        seen_paths: FxHashSet::default(),
    })
    .symbol_tables;
    let new_tables = analyze(AnalysisBatch {
        opts: CompileOpts::default(),
        files: vec![(path.clone(), "contract C {\n    address newValue;\n}\n".into())],
        seen_paths: FxHashSet::default(),
    })
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let params = HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri),
            position: Position::new(1, 12),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let mut state = GlobalState::new(ClientSocket::new_closed());
    *state.symbol_tables.write() = old_tables;
    state.analysis_version.fetch_add(1, Ordering::AcqRel);

    let mut request = std::pin::pin!(crate::handlers::hover(&mut state, params));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(request.as_mut().poll(&mut context).is_pending());

    state.analysis_version.fetch_add(1, Ordering::AcqRel);
    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(2, new_tables));
    assert!(!snapshot.publish_symbol_tables(1, SymbolTables::default()));

    let Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("hover request should complete after analysis is published");
    };
    let hover = response.unwrap().expect("new analysis should provide hover");
    assert_eq!(hover.range.unwrap().start, Position::new(1, 12));
    let HoverContents::Markup(contents) = hover.contents else {
        panic!("hover should use markdown");
    };
    assert!(contents.value.contains("address newValue"));
}
