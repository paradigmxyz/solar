//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=disasm-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
// The dump only makes the tester run FileCheck; it reads the source map.

// DEBUG-005: identical terminal blocks, such as two `return 1;` or two returns of the same
// `abi.encodeCall`, merge into one block. Its location is the merge of its sites, which the legacy
// source map writes as `-1`, so no merged return has an entry of its own.

// Offsets are bytes from `contract C`. An entry that changes the location writes its start, so a
// location with no instruction never appears.

// CHECK: "srcmap-runtime":"[[#C:]]:
// The shared block of the merged `return 1;`.
// CHECK: -1:-1:-1;
// `x == 3`
// CHECK: {{[;"]}}[[#C+85]]{{[:;]}}
// `x == 4`
// CHECK: {{[;"]}}[[#C+139]]{{[:;]}}
// `return 2;`
// CHECK: {{[;"]}}[[#C+189]]{{[:;]}}
// `x == 3`, `x == 4` and `abi.encodeCall(this.g, (2))`
// CHECK: {{[;"]}}[[#C+285]]{{[:;]}}
// CHECK: {{[;"]}}[[#C+365]]{{[:;]}}
// CHECK: {{[;"]}}[[#C+448]]{{[:;]}}
// The shared block of the merged `abi.encodeCall` returns.
// CHECK: -1:-1:-1;
// Both `return 1;` and both `return abi.encodeCall(this.g, (1));`. The entries of an unshared
// return come after the code that follows it.
// CHECK-NOT: {{[;"]}}[[#C+107]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+161]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+307]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+387]]{{[:;]}}

contract C {
    function g(uint256 x) external pure returns (uint256) {
        if (x == 3) {
            return 1;
        }
        if (x == 4) {
            return 1;
        }
        return 2;
    }

    function enc(uint256 x) external view returns (bytes memory) {
        if (x == 3) {
            return abi.encodeCall(this.g, (1));
        }
        if (x == 4) {
            return abi.encodeCall(this.g, (1));
        }
        return abi.encodeCall(this.g, (2));
    }
}
