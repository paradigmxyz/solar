//@compile-flags: -Ztypeck
// Valid operations on calldata (reading is allowed)

struct S {
    uint256 x;
}

contract Test {
    uint256 state;
    uint256 idx;
    
    function testCalldataRead(uint256[] calldata arr) external {
        uint256 x = arr[idx];
        state = x;
    }
    
    function testCalldataStructRead(S calldata s) external {
        uint256 x = s.x;
        state = x;
    }
    
    function testCalldataReassign(uint256[] calldata arr, uint256[] calldata other) external pure {
        arr = other;
    }
    
    function testCalldataLength(uint256[] calldata arr) external {
        state = arr.length;
    }
}
