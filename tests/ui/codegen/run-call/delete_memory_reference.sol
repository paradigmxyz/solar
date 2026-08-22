//@ run-call: array() => 0, 1, 7
//@ run-call: bytesValue() => 0, 2, 0xabcd

contract DeleteMemoryReference {
    function array() external pure returns (uint256, uint256, uint256) {
        uint256[] memory a = new uint256[](1);
        a[0] = 7;
        uint256[] memory b = a;
        delete a;
        return (a.length, b.length, b[0]);
    }

    function bytesValue() external pure returns (uint256, uint256, bytes2) {
        bytes memory a = hex"abcd";
        bytes memory b = a;
        delete a;
        return (a.length, b.length, bytes2(b));
    }
}
