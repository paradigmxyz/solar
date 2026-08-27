//@ run-call: testAddress 1 => true
//@ run-call: testChainId 1 => true

contract C {
    function testAddress(uint256 x) external view returns (bool) {
        address self = address(this);
        uint256 y;
        if ((x ^ uint160(self)) & 1 == 0) {
            y = x + 1;
        } else {
            y = x + 2;
        }
        return y != 0 && self != address(0);
    }

    function testChainId(uint256 x) external view returns (bool) {
        uint256 chainId = block.chainid;
        uint256 y;
        if ((x ^ chainId) & 1 == 0) {
            y = x + 1;
        } else {
            y = x + 2;
        }
        return y != 0 && chainId == 1;
    }
}
