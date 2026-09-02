//@ codegen-matrix: standard
//@ run-call: testDynamicIndex() => 1, 2, 1
//@ run-call: testConstantIndex() => 1, 2, 1
//@ run-call: testFromMemory() => 1, 2, 1
//@ run-call: testNested() => 8, 9, 1
// Whole-struct assignment into a fixed-size storage array element writes the
// members, not the memory pointer of the temporary.
contract StorageFixedStructArrayAssign {
    struct It {
        uint256 id;
        uint256 value;
        bool active;
    }

    It[4] fixedArr;
    It[2][2] nested;
    uint256 index = 1;

    function setUp() public {
        fixedArr[index] = It(1, 2, true);
    }

    function read(uint256 base) internal view returns (uint256 a, uint256 b, uint256 c) {
        assembly {
            a := sload(base)
            b := sload(add(base, 1))
            c := sload(add(base, 2))
        }
    }

    function testDynamicIndex() public view returns (uint256, uint256, uint256) {
        return read(3);
    }

    function testConstantIndex() public returns (uint256, uint256, uint256) {
        fixedArr[3] = It(1, 2, true);
        return read(9);
    }

    function testFromMemory() public returns (uint256, uint256, uint256) {
        It memory m = It(1, 2, true);
        fixedArr[index + 1] = m;
        return read(6);
    }

    function testNested() public returns (uint256, uint256, uint256) {
        nested[index][index] = It(8, 9, true);
        return read(12 + 9);
    }
}
