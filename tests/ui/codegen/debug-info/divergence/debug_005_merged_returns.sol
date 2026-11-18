//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
// The dump makes the tester run FileCheck and shows the locations of the code around the checks.

// DEBUG-005: identical terminal blocks, such as two `return 1;` or two returns of the same
// `abi.encodeCall`, merge into one block. Its location is the merge of its sites, which the legacy
// source map writes as `-1`, so no merged return has an entry of its own.

// Offsets are bytes from `contract C`. An entry that changes the location writes its start, so a
// location with no instruction never appears anywhere in the map.

// CHECK: "srcmap-runtime":"[[#C:]]:
// The shared block of the merged `return 1;`.
// CHECK: -1:-1:-1;
// Both `return 1;` and both `return abi.encodeCall(this.g, (1));`.
// CHECK-NOT: {{[;"]}}[[#C+107]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+161]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+307]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+387]]{{[:;]}}
// CHECK: "sourceList"
// The IR below shows the neighbouring code has locations.
// `x == 3`
// CHECK: {{[=\[ ]}}[[#C+85]]..
// `x == 4`
// CHECK: {{[=\[ ]}}[[#C+139]]..
// `return 2;`
// CHECK: {{[=\[ ]}}[[#C+189]]..
// The shared `return` instruction carries both `return 1;` sites.
// CHECK: return !metadata(spans=[[[#C+107]]..{{[0-9]+}}, [[#C+161]]..{{[0-9]+}}]
// `x == 3`, `x == 4` and `abi.encodeCall(this.g, (2))`
// CHECK: {{[=\[ ]}}[[#C+285]]..
// CHECK: {{[=\[ ]}}[[#C+365]]..
// CHECK: {{[=\[ ]}}[[#C+448]]..
// The shared `return` instruction carries both `abi.encodeCall` sites.
// CHECK: return !metadata(spans=[[[#C+307]]..{{[0-9]+}}, [[#C+387]]..{{[0-9]+}}]

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
