//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=disasm-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// DEBUG-005: identical terminal blocks, such as two `return 1;` or two returns of the same
// `abi.encodeCall`, merge into one block. Its location is the merge of its sites, which the legacy
// source map writes as `-1`, so no merged return has an entry of its own.

// Offsets are bytes from `contract C`. An entry that changes the location writes its start, so a
// location with no instruction never appears.

// CHECK: "srcmap-runtime":"[[#C:]]:
// The shared blocks of the merged returns.
// CHECK: -1:-1:-1;
// `x == 3`
// CHECK: {{[;"]}}[[#C+85]]{{[:;]}}
// `return 1;`
// CHECK-NOT: {{[;"]}}[[#C+107]]{{[:;]}}
// `x == 4`
// CHECK: {{[;"]}}[[#C+139]]{{[:;]}}
// `return 1;`
// CHECK-NOT: {{[;"]}}[[#C+161]]{{[:;]}}
// `return 2;`
// CHECK: {{[;"]}}[[#C+189]]{{[:;]}}
// `x == 3`
// CHECK: {{[;"]}}[[#C+285]]{{[:;]}}
// `return abi.encodeCall(this.g, (1));`
// CHECK-NOT: {{[;"]}}[[#C+307]]{{[:;]}}
// `x == 4`
// CHECK: {{[;"]}}[[#C+365]]{{[:;]}}
// `return abi.encodeCall(this.g, (1));`
// CHECK-NOT: {{[;"]}}[[#C+387]]{{[:;]}}
// `abi.encodeCall(this.g, (2))`
// CHECK: {{[;"]}}[[#C+448]]{{[:;]}}

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
