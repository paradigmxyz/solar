//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=disasm-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// DEBUG-003: a call in tail position has no instruction of its own when the code before it jumps
// to the callee directly, as the `require` does for `_set(x);`, or when the caller jumps past a
// function that only forwards, as `forward` does past `_forward`. The forwarder has no entry
// label, and its `return _twice(x);` has no instruction.

// Offsets are bytes from `contract C`. An entry that changes the location writes its start, so a
// location with no instruction never appears.

// CHECK: "srcmap-runtime":"[[#C:]]:
// `require(x != 0);`
// CHECK: {{[;"]}}[[#C+254]]{{[:;]}}
// `_set(x);`
// CHECK-NOT: {{[;"]}}[[#C+279]]{{[:;]}}
// `total = x;`
// CHECK: {{[;"]}}[[#C+343]]{{[:;]}}
// `_forward(x)`
// CHECK: {{[;"]}}[[#C+185]]{{[:;]}}
// `function _forward(...)` and `return _twice(x);`
// CHECK-NOT: {{[;"]}}[[#C+365]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+436]]{{[:;]}}
// CHECK-NOT: {{[;"]}}[[#C+443]]{{[:;]}}
// `x * 2`
// CHECK: {{[;"]}}[[#C+541]]{{[:;]}}
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
