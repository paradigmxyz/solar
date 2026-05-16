contract C {
    function f() public returns (uint256 x) {
        // Ported from solc test/libyul/yulSyntaxTests/for_statement_2.yul.
        assembly {
            { let limit := calldatasize() for { let i := 0 } lt(i, limit) { i := add(i, 1) } { x := add(x, 2) } }
        }
    }
}
