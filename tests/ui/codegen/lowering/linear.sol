//@ check-pass
//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract Linear {
    function add(uint256 x, uint256 y) public pure returns (uint256) {
        return x + y;
    }

    function sub(uint256 x, uint256 y) public pure returns (uint256) {
        return x - y;
    }

    function add_one(uint256 x) public pure returns (uint256) {
        return x + 1;
    }
}
