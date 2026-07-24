//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// A same-contract internal function whose body contains a loop, called by bare
// ident, must be lowered as a real `internal_call` rather than inlined. The SSA
// inline path (`lower_library_body_simple`) can't model a loop's induction
// variable, so inlining produced a loop whose counter never advanced — an
// infinite loop that ran out of gas. Runtime-verified against solc: run(5) == 10.
contract C {
    // CHECK-LABEL: fn @sumTo
    // CHECK: [[I:v[0-9]+]] = mload {{v[0-9]+}} !metadata(memory=internal_frame)
    // CHECK: lt [[I]], arg0
    // CHECK: add {{v[0-9]+}}, 1
    function sumTo(uint256 n) internal pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            s += i;
        }
    }

    // CHECK-LABEL: fn @run
    // CHECK: [[SUM:v[0-9]+]] = internal_call @sumTo, 1, arg0
    function run(uint256 n) public pure returns (uint256) {
        return sumTo(n);
    }
}
