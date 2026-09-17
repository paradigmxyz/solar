//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ run-call: first 0 => 5
//@ run-call: first 3 => 8
//@ run-call: second 3 => 11
//@ run-call: third 3 => 14
//@ run-call: combined 3 => 19
//@ run-call: bounded 0 => 7
//@ run-call: bounded 255 => 19

// Different calls share the same continuation code. A continuation consumes the result from
// its own call and must preserve any caller value underneath the hidden return address.
contract ContinuationCalls {
    function first(uint256 x) external pure returns (uint256) {
        return sum(x, 1) + 5;
    }

    function second(uint256 x) external pure returns (uint256) {
        return sum(x, 2) + 5;
    }

    function third(uint256 x) external pure returns (uint256) {
        return sum(x, 3) + 5;
    }

    function combined(uint256 x) external pure returns (uint256) {
        return x + sum(x, 1) + sum(x, 2) + 7;
    }

    function bounded(uint256 x) external pure returns (uint256) {
        x &= 3;
        return x + sum(x, 1) + sum(x, 2) + 7;
    }

    function sum(uint256 x, uint256 step) internal pure returns (uint256 r) {
        for (uint256 i; i < x; ++i) r += step;
    }
}
