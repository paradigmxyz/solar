//@ codegen-matrix: standard
//@ run-call-fail: f => 0x30b1b5650000000000000000000000000000000000000000000000000000000000000001
// ported-from: test/libsolidity/semanticTests/errors/require_custom_error_can_be_parenthesized.sol

contract C {
    error MyError(uint256);
    function f() public pure returns (uint256)
    {
        require(false, (MyError(1)));
        return 42;
    }
}
