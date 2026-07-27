use super::support::RequestFixture;
use snapbox::str;

#[test]
fn shows_selectors_and_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /CodeLens.sol
        contract Token {
            uint256 public value;

            function transfer(address to, uint256 amount)
                public
                returns (uint256 result)
            {
                value = amount;
                return amount;
            }

            function callTransfer(address target) external {
                transfer(target, value);
            }
        }

        "#,
        "/CodeLens.sol",
    );

    fixture.check_code_lenses(
        "/CodeLens.sol",
        str![[r#"
0:9 references=0 command=<none>
1:19 references=2 command=solar.showReferences
1:19 selector=0x3fa4f245 command=solar.copySelector
2:13 references=1 command=solar.showReferences
2:13 selector=0xa9059cbb command=solar.copySelector
2:30 references=0 command=<none>
2:42 references=2 command=solar.showReferences
9:13 references=0 command=<none>
9:13 selector=0xeec990f2 command=solar.copySelector
9:34 references=1 command=solar.showReferences

"#]],
    );
}

#[test]
fn shows_direct_inheritance_counts() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hierarchy.sol
        contract Base {}
        contract Mid is Base {}
        contract Leaf is Mid {}
        "#,
        "/Hierarchy.sol",
    );

    fixture.check_code_lenses(
        "/Hierarchy.sol",
        str![[r#"
0:9 references=1 command=solar.showReferences
0:9 inheritance=1 derived contract command=solar.showTypeHierarchy
1:9 references=1 command=solar.showReferences
1:9 inheritance=1 base contract command=solar.showTypeHierarchy
1:9 inheritance=1 derived contract command=solar.showTypeHierarchy
2:9 references=0 command=<none>
2:9 inheritance=1 base contract command=solar.showTypeHierarchy

"#]],
    );
}

#[test]
fn snapshots_complete_command_protocol() {
    let fixture = RequestFixture::new(
        r#"
        //- /Protocol.sol
        contract Base {}
        contract Plain is Base {
            function target() public {}
            function callTarget() external { target(); }
        }
        "#,
        "/Protocol.sol",
    );

    fixture.check_code_lenses_json(
        "/Protocol.sol",
        str![[r#"
[
  {
    "range": {
      "start": {
        "line": 0,
        "character": 9
      },
      "end": {
        "line": 0,
        "character": 13
      }
    },
    "command": {
      "title": "1 reference",
      "command": "solar.showReferences",
      "arguments": [
        {
          "position": {
            "character": 9,
            "line": 0
          },
          "uri": "file:///Protocol.sol"
        }
      ]
    }
  },
  {
    "range": {
      "start": {
        "line": 0,
        "character": 9
      },
      "end": {
        "line": 0,
        "character": 13
      }
    },
    "command": {
      "title": "1 derived contract",
      "command": "solar.showTypeHierarchy",
      "arguments": [
        {
          "direction": "subtypes",
          "position": {
            "character": 9,
            "line": 0
          },
          "uri": "file:///Protocol.sol"
        }
      ]
    }
  },
  {
    "range": {
      "start": {
        "line": 1,
        "character": 9
      },
      "end": {
        "line": 1,
        "character": 14
      }
    },
    "command": {
      "title": "0 references",
      "command": ""
    }
  },
  {
    "range": {
      "start": {
        "line": 1,
        "character": 9
      },
      "end": {
        "line": 1,
        "character": 14
      }
    },
    "command": {
      "title": "1 base contract",
      "command": "solar.showTypeHierarchy",
      "arguments": [
        {
          "direction": "supertypes",
          "position": {
            "character": 9,
            "line": 1
          },
          "uri": "file:///Protocol.sol"
        }
      ]
    }
  },
  {
    "range": {
      "start": {
        "line": 2,
        "character": 13
      },
      "end": {
        "line": 2,
        "character": 19
      }
    },
    "command": {
      "title": "1 reference",
      "command": "solar.showReferences",
      "arguments": [
        {
          "position": {
            "character": 13,
            "line": 2
          },
          "uri": "file:///Protocol.sol"
        }
      ]
    }
  },
  {
    "range": {
      "start": {
        "line": 2,
        "character": 13
      },
      "end": {
        "line": 2,
        "character": 19
      }
    },
    "command": {
      "title": "0xd4b83992",
      "command": "solar.copySelector",
      "arguments": [
        "0xd4b83992"
      ]
    }
  },
  {
    "range": {
      "start": {
        "line": 3,
        "character": 13
      },
      "end": {
        "line": 3,
        "character": 23
      }
    },
    "command": {
      "title": "0 references",
      "command": ""
    }
  },
  {
    "range": {
      "start": {
        "line": 3,
        "character": 13
      },
      "end": {
        "line": 3,
        "character": 23
      }
    },
    "command": {
      "title": "0x2872b1ff",
      "command": "solar.copySelector",
      "arguments": [
        "0x2872b1ff"
      ]
    }
  }
]
"#]],
    );
}

#[test]
fn merges_reference_counts_for_imported_declarations() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Shared.sol
        contract $1Base {
            function $2ping() public {}
        }

        //- /first/Main.sol
        import "../Shared.sol";
        contract First {
            function useBase() external {
                Base value;
                value.ping();
            }
        }

        //- /second/Main.sol
        import "../Shared.sol";
        contract Second {
            function useBase() external {
                Base value;
                value.ping();
            }
        }
        "#,
        &["/first/Main.sol", "/second/Main.sol"],
    );

    fixture.check_code_lenses(
        "/Shared.sol",
        str![[r#"
0:9 references=2 command=solar.showReferences
1:13 references=2 command=solar.showReferences
1:13 selector=0x5c36b186 command=solar.copySelector

"#]],
    );
    fixture.check_references(
        "$1",
        false,
        str![[r#"
/first/Main.sol:3:8 Base value;
/second/Main.sol:3:8 Base value;

"#]],
    );
    fixture.check_references(
        "$2",
        false,
        str![[r#"
/first/Main.sol:4:14 value.ping();
/second/Main.sol:4:14 value.ping();

"#]],
    );
}

#[test]
fn rejects_references_from_conflicting_source_snapshots() {
    let source = r#"
        //- /Target.sol
        library Target {
            function $1target() external pure {}
        }

        //- /Caller.sol open
        import {Target} from "./Target.sol";

        contract C {
            function caller() external pure {
                Target.target();
            }
        }

        //- /Root.sol
        import "./Caller.sol";
        "#;
    let disk_contents = r#"import {Target} from "./Target.sol";

contract C {
    function caller() external pure {

        Target.target();
    }
}
"#;

    for paths in [["/Root.sol", "/Caller.sol"], ["/Caller.sol", "/Root.sol"]] {
        let fixture = RequestFixture::new_in_batches_with_stale_disk(
            source,
            "/Caller.sol",
            disk_contents,
            &paths,
        );

        fixture.check_code_lenses(
            "/Target.sol",
            str![[r#"
1:13 selector=0xd4b83992 command=solar.copySelector

"#]],
        );
        fixture.check_references("$1", false, "<none>\n");
    }
}

#[test]
fn selectors_cover_public_external_functions_and_getters_only() {
    let fixture = RequestFixture::new(
        r#"
        //- /Selectors.sol
        contract Selectors {
            uint256 public value;
            uint256 private hidden;

            function externalFn(uint256 input) external {}
            function publicFn() public {}
            function internalFn() internal {}
            function privateFn() private {}
            constructor() {}
            fallback() external {}
            receive() external payable {}
        }
        "#,
        "/Selectors.sol",
    );

    fixture.check_code_lenses(
        "/Selectors.sol",
        str![[r#"
0:9 references=0 command=<none>
1:19 references=0 command=<none>
1:19 selector=0x3fa4f245 command=solar.copySelector
2:20 references=0 command=<none>
3:13 references=0 command=<none>
3:13 selector=0x43a389ef command=solar.copySelector
3:32 references=0 command=<none>
4:13 references=0 command=<none>
4:13 selector=0x5e6858dd command=solar.copySelector
5:13 references=0 command=<none>
6:13 references=0 command=<none>
7:4 references=0 command=<none>
8:4 references=0 command=<none>
9:4 references=0 command=<none>

"#]],
    );
}

#[test]
fn skips_selectors_for_invalid_signatures() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Invalid.sol
        contract Invalid {
            function bad(Unknown value) external {}
        }
        "#,
        "/Invalid.sol",
    );

    fixture.check_code_lenses(
        "/Invalid.sol",
        str![[r#"
0:9 references=0 command=<none>
1:13 references=0 command=<none>
1:25 references=0 command=<none>

"#]],
    );
}

#[test]
fn excludes_yul_declarations_and_solidity_locals_and_returns() {
    let fixture = RequestFixture::new(
        r#"
        //- /Yul.sol
        contract Yul {
            function outer(uint256 input) external pure returns (uint256 output) {
                uint256 local = input;
                assembly {
                    function inner(y) -> z { z := y }
                    output := inner(local)
                }
            }
        }
        "#,
        "/Yul.sol",
    );

    fixture.check_code_lenses(
        "/Yul.sol",
        str![[r#"
0:9 references=0 command=<none>
1:13 references=0 command=<none>
1:13 selector=0x94209e43 command=solar.copySelector
1:27 references=1 command=solar.showReferences

"#]],
    );
}

#[test]
fn preserves_titles_without_client_commands() {
    let fixture = RequestFixture::new(
        r#"
        //- /Plain.sol
        contract Plain {
            function target() public {}
            function callTarget() external { target(); }
        }
        "#,
        "/Plain.sol",
    );

    fixture.check_code_lenses_without_commands(
        "/Plain.sol",
        str![[r#"
0:9 references=0 command=<none>
1:13 references=1 command=<none>
1:13 selector=0xd4b83992 command=<none>
2:13 references=0 command=<none>
2:13 selector=0x2872b1ff command=<none>

"#]],
    );
}
