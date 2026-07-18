//@compile-flags: -Zcodegen --emit=evm-ir-runtime

contract Test {
    function select(address account, uint256 value) external pure returns (uint256) {
        if (account == address(1)) return value + 1;
        if (account == address(2)) return value + 2;
        if (account == address(3)) return value + 3;
        if (account == address(4)) return value + 4;
        if (account == address(5)) return value + 5;
        return value;
    }
}
