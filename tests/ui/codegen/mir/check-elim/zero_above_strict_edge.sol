//@ codegen-matrix: standard
//@ run-call: f 0 => 0
//@ run-call: f 4 => 2
//@ run-call-fail: f 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012

// The first iteration leaves `1 / x` known zero on the path where `x` also lies above a strict
// edge. That point is unreachable, and its range must stay nonempty so that the division bound
// does not divide by zero.
contract C {
    function f(uint256 x) public pure returns (uint256) {
        for (uint256 y = x / 2; y < x; y = x + 1 / x) x = y;
        return x;
    }
}
