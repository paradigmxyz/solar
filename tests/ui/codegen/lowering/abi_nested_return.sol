//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract AbiNestedReturn {
    struct Pair {
        uint256 a;
        uint256 b;
    }

    function structArray(uint256 x) public pure returns (Pair[] memory) {
        Pair[] memory out = new Pair[](1);
        out[0] = Pair(x, x + 1);
        return out;
    }

    function nestedArray(uint256 n) public pure returns (uint256[][] memory) {
        uint256[][] memory out = new uint256[][](1);
        out[0] = new uint256[](n);
        return out;
    }
}
