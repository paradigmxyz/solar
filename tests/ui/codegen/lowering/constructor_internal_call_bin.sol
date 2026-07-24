//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir --pretty-json

contract ConstructorInternalCallBin {
    uint256 public value;

    constructor(uint256 x) {
        value = helper(x & 7);
    }

    function helper(uint256 n) internal pure returns (uint256) {
        if (n == 0) {
            return 1;
        }
        return n * 11 + helper(n - 1);
    }
}
