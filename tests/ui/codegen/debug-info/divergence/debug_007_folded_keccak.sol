//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
// The dump makes the tester run FileCheck and shows the locations of the code around the checks.

// DEBUG-007: `keccak256` of `abi.encode` or `abi.encodePacked` of constants is folded at compile
// time. The hash is a push in the code that reads it, so the call and its arguments have no entry,
// while the same call on a runtime value does. `solc` folds only a string literal argument.

// Offsets are bytes from `contract C`. An entry that changes the location writes its start, so a
// location with no instruction never appears anywhere in the map.

// CHECK: "srcmap-runtime":"[[#C:]]:
// `keccak256(abi.encodePacked("a", "b"))`, `abi.encodePacked("a", "b")` and its `"a"` and `"b"`
// CHECK-NOT: {{[;"]}}[[#C+133]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+143]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+160]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+165]]{{[:;"]}}
// `keccak256(abi.encode(uint256(1), uint256(2)))`, `abi.encode(...)` and its `uint256(1)`, `1`,
// `uint256(2)` and `2`
// CHECK-NOT: {{[;"]}}[[#C+251]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+261]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+272]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+280]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+284]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+292]]{{[:;"]}}
// CHECK: "sourceList"
// The IR below shows the neighbouring code has locations.
// `return keccak256(abi.encodePacked("a", "b"));`
// CHECK: {{[=\[ ]}}[[#C+126]]..
// `return keccak256(abi.encode(uint256(1), uint256(2)));`
// CHECK: {{[=\[ ]}}[[#C+244]]..
// `return SLOT;`
// CHECK: {{[=\[ ]}}[[#C+368]]..
// `keccak256(abi.encode(x))` and `return keccak256(abi.encode(x));`
// CHECK: {{[=\[ ]}}[[#C+469]]..
// CHECK: {{[=\[ ]}}[[#C+462]]..

contract C {
    bytes32 constant SLOT = keccak256("a.b.c");

    function packed() external pure returns (bytes32) {
        return keccak256(abi.encodePacked("a", "b"));
    }

    function encoded() external pure returns (bytes32) {
        return keccak256(abi.encode(uint256(1), uint256(2)));
    }

    function named() external pure returns (bytes32) {
        return SLOT;
    }

    function runtime(uint256 x) external pure returns (bytes32) {
        return keccak256(abi.encode(x));
    }
}
