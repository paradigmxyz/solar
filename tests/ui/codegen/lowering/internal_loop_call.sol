//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

// A same-contract internal function whose body contains a loop, called by bare
// ident, must be lowered as a real `internal_call` rather than inlined. The SSA
// inline path (`lower_library_body_simple`) can't model a loop's induction
// variable, so inlining produced a loop whose counter never advanced — an
// infinite loop that ran out of gas. Runtime-verified against solc: run(5) == 10.
contract C {
    function sumTo(uint256 n) internal pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            s += i;
        }
    }

    function run(uint256 n) public pure returns (uint256) {
        return sumTo(n);
    }
}
