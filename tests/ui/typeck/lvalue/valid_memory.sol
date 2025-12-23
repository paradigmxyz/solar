//@compile-flags: -Ztypeck
// Valid lvalue assignments for memory variables

struct S {
    uint256 x;
    uint256 y;
}

contract Test {
    function testLocalVar() external pure {
        uint256 local;
        uint256 x;
        local = x;
    }
    
    function testMemoryStruct() external pure {
        S memory s;
        uint256 x;
        s.x = x;
        s.y = x;
    }
    
    function testMemoryStructReassign() external pure {
        S memory s;
        S memory other;
        s = other;
    }
    
    function testTupleAssign() external pure {
        uint256 x;
        uint256 y;
        (x, y) = (y, x);
    }
    
    function testTuplePartialAssign() external pure {
        uint256 x;
        uint256 y;
        uint256 z;
        (x, y, z) = (z, y, x);
    }
    
    function testFunctionParam(uint256 param) external pure {
        uint256 x;
        param = x;
    }
}
