//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=disasm-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
// The dump only makes the tester run FileCheck; it reads the source map.

// DEBUG-001: a statement whose value is a constant, a copy or a folded constant expression has no
// instruction of its own. The value sits in the push or dup of the code that reads it, or in a
// stack rename, so the source map has no entry for the statement. An `if` arm or
// `catch` block made only of such statements has none either, and neither has a Yul `let` or
// assignment of a constant or a copy.

// Offsets are bytes from `contract C`. An entry that changes the location writes its start, so a
// location with no instruction never appears.

// CHECK: "srcmap-runtime":"[[#C:]]:
// `function declarations`
// CHECK: {{[;"]}}[[#C+49]]{{[:;]}}
// `uint256 r = 5;`, `uint8 a;`, `uint256 b = x;`, `b = r;`, `bytes32 slot = bytes32(SLOT);`,
// `uint256 big = 2 ** 128;` and `uint256 m = type(uint256).max - 3;`
// CHECK-NOT: {{[;"]}}[[#C+124]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+147]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+164]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+187]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+202]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+240]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+272]]{{[:;]}}
// `return b + big + m + uint256(slot) + a;`
// CHECK: {{[;"]}}[[#C+315]]{{[:;]}}
// `x == 1`
// CHECK: {{[;"]}}[[#C+418]]{{[:;]}}
// `uint256 z = 5;` and `uint256 w = z;` in the arm
// CHECK-NOT: {{[;"]}}[[#C+440]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+467]]{{[:;]}}
// `function yul`
// CHECK: {{[;"]}}[[#C+503]]{{[:;]}}
// `let q := 0x10` with its `0x10`, `let w := x` with its `x`, `q := w` and `y := q`
// CHECK-NOT: {{[;"]}}[[#C+594]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+603]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+620]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+629]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+643]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+662]]{{[:;]}}
// `function caught` and `i.p()`
// CHECK: {{[;"]}}[[#C+690]]{{[:;]}}
// CHECK: {{[;"]}}[[#C+734]]{{[:;]}}
// `uint256 z = 2;` and `uint256 w = z;` in the `catch` block
// CHECK-NOT: {{[;"]}}[[#C+763]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+790]]{{[:;]}}

interface I {
    function p() external;
}

contract C {
    uint256 constant SLOT = 7;

    function declarations(uint256 x) external pure returns (uint256) {
        uint256 r = 5;
        uint8 a;
        uint256 b = x;
        b = r;
        bytes32 slot = bytes32(SLOT);
        uint256 big = 2 ** 128;
        uint256 m = type(uint256).max - 3;
        return b + big + m + uint256(slot) + a;
    }

    function arm(uint256 x) external pure {
        if (x == 1) {
            uint256 z = 5;
            uint256 w = z;
        }
    }

    function yul(uint256 x) external pure returns (uint256 y) {
        assembly {
            let q := 0x10
            let w := x
            q := w
            y := q
        }
    }

    function caught(I i) external {
        try i.p() {} catch {
            uint256 z = 2;
            uint256 w = z;
        }
    }
}
