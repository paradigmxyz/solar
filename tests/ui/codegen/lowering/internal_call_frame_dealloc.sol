//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime --pretty-json

contract InternalCallFrameDealloc {
    function f(uint256 x) public pure returns (uint256) {
        return sum(x) + sum(x + 1);
    }

    function sum(uint256 x) internal pure returns (uint256) {
        if (x == 0) {
            return 0;
        }
        return x + sum(x - 1);
    }
}
