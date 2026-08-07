//@ run-call: f => 16, 32, 64
//@ run-call: g => 4
// ported-from: test/libsolidity/semanticTests/modifiers/function_return_parameter_complex.sol

contract ModifierReturnParameterComplex {
    uint256 private x;

    modifier alwaysZeros(uint256 a, uint256 b) {
        x++;
        _;
        require(a == 0, "a is not zero");
        require(b == 0, "b is not zero");
    }

    function f() external alwaysZeros(r1, r3) returns (uint256 r1, uint256 r2, uint256 r3) {
        r1 = 16;
        r2 = 32;
        r3 = 64;
    }

    function g()
        external
        alwaysZeros(r, r)
        alwaysZeros(r, r)
        alwaysZeros(r + r, r - r)
        alwaysZeros(r * r, r & r)
        returns (uint256 r)
    {
        r = x;
    }
}
