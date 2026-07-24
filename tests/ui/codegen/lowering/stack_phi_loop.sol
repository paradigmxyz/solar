//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime --pretty-json

contract StackPhiLoop {
    function loopCarried(uint256 n, bool flag) public pure returns (uint256) {
        uint256 step = flag ? 7 : 11;
        uint256 acc = 0;
        for (uint256 i = 0; i < n; i++) {
            acc += i * 3 + step;
        }
        return acc;
    }

    function sequential(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 acc = 0;
        for (uint256 i = 0; i < a; i++) {
            acc += i + 1;
        }
        for (uint256 j = 0; j < b; j++) {
            acc += j * 2 + 3;
        }
        return acc;
    }

    function nested(uint256 outer, uint256 inner) public pure returns (uint256) {
        uint256 acc = 0;
        for (uint256 i = 0; i < outer; i++) {
            for (uint256 j = 0; j < inner; j++) {
                acc += i + j + 1;
            }
        }
        return acc;
    }
}
