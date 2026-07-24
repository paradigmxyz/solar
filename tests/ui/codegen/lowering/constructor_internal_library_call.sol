//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir

library ConstructorLibrary {
    function helper(uint256 n) internal pure returns (uint256) {
        if (n == 0) {
            return 1;
        }
        return n * 7 + helper(n - 1);
    }
}

contract ConstructorInternalLibraryCall {
    uint256 public value;

    constructor(uint256 x) {
        value = ConstructorLibrary.helper(x & 7);
    }
}
