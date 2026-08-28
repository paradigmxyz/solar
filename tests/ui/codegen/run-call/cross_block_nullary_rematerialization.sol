//@ run-call: testAddress 0 => true
//@ run-call: testAddress 1 => true
//@ run-call: testChainId 0 => true
//@ run-call: testChainId 1 => true
//@ run-call: testCallValue 0; value=7 => 7
//@ run-call: testCallValue 1; value=7 => 7
//@ run-call: testCaller 0 => true
//@ run-call: testCaller 1 => true
//@ run-call: testBlockNumber 0 => true
//@ run-call: testBlockNumber 1 => true

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

    function testCallValue(uint256 x) external payable returns (uint256) {
        uint256 callValue = msg.value;
        uint256 y;
        if ((x ^ callValue) & 1 == 0) {
            y = x + 1;
        } else {
            y = x + 2;
        }
        return y != 0 ? callValue : 0;
    }

    function testCaller(uint256 x) external view returns (bool) {
        address caller = msg.sender;
        uint256 y;
        if ((x ^ uint160(caller)) & 1 == 0) {
            y = x + 1;
        } else {
            y = x + 2;
        }
        return y != 0 && caller == msg.sender;
    }

    function testBlockNumber(uint256 x) external view returns (bool) {
        uint256 number = block.number;
        uint256 y;
        if ((x ^ number) & 1 == 0) {
            y = x + 1;
        } else {
            y = x + 2;
        }
        return y != 0 && number == block.number;
    }
}
