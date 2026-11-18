//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
// The dump makes the tester run FileCheck and shows the locations of the code around the checks.

// DEBUG-004: identical reverts in one function share a revert block. Its location is the merge
// of its sites, which the legacy source map writes as `-1`, so no site has an entry of its own.

// Offsets are bytes from `contract C`. An entry that changes the location writes its start, so a
// location with no instruction never appears anywhere in the map.

// CHECK: "srcmap-runtime":"[[#C:]]:
// The shared block of both reverts.
// CHECK: -1:-1:-1;
// `revert E(1);` and its `E(1)`, twice.
// CHECK-NOT: {{[;"]}}[[#C+116]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+123]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+149]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+156]]{{[:;"]}}
// CHECK: "sourceList"
// The IR below shows the neighbouring code has locations.
// `x == 1`
// CHECK: {{[=\[ ]}}[[#C+108]]..
// `x == 2`
// CHECK: {{[=\[ ]}}[[#C+141]]..
// `return x;`
// CHECK: {{[=\[ ]}}[[#C+170]]..
// The shared `revert` instruction carries both sites.
// CHECK: revert !metadata(spans=[[[#C+116]]..{{[0-9]+}}, [[#C+149]]..{{[0-9]+}}]

contract C {
    error E(uint256);

    function f(uint256 x) external pure returns (uint256) {
        if (x == 1) revert E(1);
        if (x == 2) revert E(1);
        return x;
    }
}
