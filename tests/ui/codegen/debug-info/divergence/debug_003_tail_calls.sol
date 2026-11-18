//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
// The dump makes the tester run FileCheck and shows the locations of the code around the checks.

// DEBUG-003: a call in tail position that needs no argument setup has no instruction of its own
// when the code before it jumps or falls into the callee, as the `require` does for `_set(x);`,
// or when the caller jumps past a function that only forwards, as `forward` does past
// `_forward`. The forwarder has no entry label, and its `return _twice(x);` has no instruction.

// Offsets are bytes from `contract C`. An entry that changes the location writes its start, so a
// location with no instruction never appears anywhere in the map.

// CHECK: "srcmap-runtime":"[[#C:]]:
// `_set(x);`
// CHECK-NOT: {{[;"]}}[[#C+279]]{{[:;"]}}
// `function _forward(...)` and `return _twice(x);`
// CHECK-NOT: {{[;"]}}[[#C+365]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+436]]{{[:;"]}}
// CHECK-NOT: {{[;"]}}[[#C+443]]{{[:;"]}}
// CHECK: "sourceList"
// The IR below shows the neighbouring code has locations.
// `_forward(x)`
// CHECK: {{[=\[ ]}}[[#C+185]]..
// `require(x != 0);`
// CHECK: {{[=\[ ]}}[[#C+254]]..
// `total = x;`
// CHECK: {{[=\[ ]}}[[#C+343]]..
// `x * 2`
// CHECK: {{[=\[ ]}}[[#C+541]]..
contract C {
    uint256 public total;

    function set(uint256 x) external {
        _init(x);
    }

    function forward(uint256 x) external pure returns (uint256) {
        return _forward(x);
    }

    function _init(uint256 x) internal {
        require(x != 0);
        _set(x);
    }

    function _set(uint256 x) internal {
        total = x;
    }

    function _forward(uint256 x) internal pure returns (uint256) {
        return _twice(x);
    }

    function _twice(uint256 x) internal pure returns (uint256) {
        return x * 2;
    }
}
