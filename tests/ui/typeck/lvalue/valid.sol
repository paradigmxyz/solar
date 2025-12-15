//@compile-flags: -Ztypeck
// Test cases that should NOT produce lvalue errors

struct S {
    uint256 x;
}

contract Test {
    uint256 public state;
    uint256[] dynamicArray;
    mapping(uint256 => uint256) map;
    S structVar;
    uint256 idx;

    function testStateVar() external {
        uint256 x = state;
        state = x;
    }

    function testLocalVar() external {
        uint256 local;
        uint256 x;
        local = x;
    }

    function testStorageArray() external {
        uint256 x = state;
        dynamicArray[idx] = x;
    }

    function testMapping() external {
        uint256 x = state;
        map[idx] = x;
    }

    function testStruct() external {
        uint256 x = state;
        structVar.x = x;
    }

    function testMemoryStruct() external {
        S memory s;
        uint256 x;
        s.x = x;
    }

    function testTupleAssign() external {
        uint256 x;
        uint256 y;
        (x, y) = (y, x);
    }

    function testIncrement() external {
        state++;
        ++state;
    }

    function testDelete() external {
        delete state;
        delete dynamicArray;
    }
}
