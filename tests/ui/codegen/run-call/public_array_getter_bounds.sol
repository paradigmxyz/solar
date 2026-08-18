//@ run-call: fixedValues(uint256) 0 => 0
//@ run-call-fail: fixedValues(uint256) 3
//@ run-call-fail: dynamicValues(uint256) 0
//@ run-call: nested(uint256,uint256) 7, 1 => 0
//@ run-call-fail: nested(uint256,uint256) 7, 2
//@ run-call-fail: explicitFixed(uint256) 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

contract C {
    uint256[3] public fixedValues;
    uint256[] public dynamicValues;
    mapping(uint256 => uint256[2]) public nested;

    function explicitFixed(uint256 index) external view returns (uint256) {
        return fixedValues[index];
    }
}
