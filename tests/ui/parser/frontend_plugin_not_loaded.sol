//@ compile-flags: --emit=abi
contract C {
    function f() external pure returns (uint256) {
        return @example.rotl(1, 2); //~ ERROR: no frontend plugins are loaded
    }
}
