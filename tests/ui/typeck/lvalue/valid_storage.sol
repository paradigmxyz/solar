//@compile-flags: -Ztypeck
// TODO: `mismatched types` errors on integer literals are a current limitation of solar
// Valid lvalue assignments for storage variables

contract Test {
    uint256 state;
    uint256[] dynamicArray;
    mapping(uint256 => uint256) map;
    uint256 idx;
    
    function testStateVar() external {
        uint256 x = state;
        state = x;
    }
    
    function testStorageArrayElement() external {
        uint256 x = state;
        dynamicArray[idx] = x;
    }
    
    function testMapping() external {
        uint256 x = state;
        map[idx] = x;
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
