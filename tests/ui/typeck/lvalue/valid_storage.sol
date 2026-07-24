//@ check-pass
// Valid lvalue assignments for storage variables

contract Test {
    uint256 state;
    uint256[] dynamicArray;
    mapping(uint256 => uint256) map;
    mapping(address => mapping(address => bool)) nestedMap;
    uint256 idx;
    
    function testStateVar() external {
        uint256 x = state;
        state = x;
    }
    
    function testStorageArrayElement() external {
        uint256 x = state;
        dynamicArray[idx] = x;
    }

    function testStorageArrayMethods() external {
        dynamicArray.push(1);
        uint256 x = dynamicArray.push();
        dynamicArray.pop();
        x;
    }
    
    function testMapping() external {
        uint256 x = state;
        map[idx] = x;
    }

    function testNestedMapping() external {
        nestedMap[msg.sender][address(this)] = true;
    }
    
    function testIncrement() external {
        state++;
        ++state;
        state--;
        --state;
    }
    
    function testCompoundAssignment() external {
        uint256 x = state;
        state += x;
        state -= x;
        state *= x;
        state /= x;
    }
    
    function testDelete() external {
        delete state;
        delete dynamicArray;
    }
}
