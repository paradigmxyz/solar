//@compile-flags: -Zcodegen -Zdump=mir

contract MemoryFixedArrayAlloc {
    struct Holder {
        uint256[3] values;
    }

    function guardedFix(uint256 i) public pure returns (uint256) {
        uint256[3] memory x;
        return x[i];
    }

    function structArr(uint256 i) public pure returns (uint256) {
        Holder memory h;
        return h.values[i];
    }

    function nested(uint256 i, uint256 j) public pure returns (uint256) {
        uint256[2][3] memory x;
        x[0][0] = 1;
        return x[i][j];
    }

    function fmpIntegrity() public pure returns (uint256, uint256) {
        uint256[3] memory x;
        x[2] = 7;
        uint256[] memory y = new uint256[](1);
        y[0] = 9;
        return (x[2], y[0]);
    }

    function literal() public pure returns (uint256) {
        uint256[3] memory x = [uint256(1), uint256(2), uint256(3)];
        return x[2];
    }
}

contract NamedReturnAndDelete {
    // A named fixed-array return points at real zeroed memory, not scratch.
    function namedReturn() public pure returns (uint256[3] memory x, uint256 m) {
        x[0] = 1;
        x[2] = 3;
        bytes memory b = new bytes(32);
        b[0] = 0xEE;
        m = uint8(b[0]);
    }

    // `delete` zeroes the elements in place; the pointer stays valid.
    function deleteInPlace() public pure returns (uint256, uint256) {
        uint256[3] memory x;
        x[0] = 5;
        x[1] = 6;
        x[2] = 7;
        delete x;
        x[2] = 9;
        return (x[0], x[2]);
    }
}
