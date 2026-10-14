//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract MirAllocOps {
    // CHECK-LABEL: fn @fixedArray{{[( ]}}
    // CHECK: = alloc 64
    // CHECK: memory_zero {{v[0-9]+}}, 64
    function fixedArray(uint256 value) external pure returns (uint256) {
        uint256[2] memory words;
        words[0] = value;
        return words[0];
    }

    // CHECK-LABEL: fn @dynamic{{[( ]}}
    // CHECK: = alloc
    function dynamic(bytes calldata data) external pure returns (bytes memory) {
        return data;
    }

    // CHECK-LABEL: fn @frameShadow{{[( ]}}
    // CHECK: [[RET_0:v[0-9]+]] = insert_value [[RET_TY:struct[0-9]+]], undef [[RET_TY]], 0, 1
    // CHECK: [[RET_1:v[0-9]+]] = insert_value [[RET_TY]], [[RET_0]], 1, 2
    // CHECK: [[RET_2:v[0-9]+]] = insert_value [[RET_TY]], [[RET_1]], 2, 3
    // CHECK: [[RET_3:v[0-9]+]] = insert_value [[RET_TY]], [[RET_2]], 3, 4
    // CHECK: ret [[RET_3]]
    function frameShadow()
        external
        pure
        returns (uint256, uint256, uint256, uint256)
    {
        return (1, 2, 3, 4);
    }

    // CHECK-LABEL: fn @rawAssembly{{[( ]}}
    // CHECK: = load 64
    // CHECK: store 64,
    function rawAssembly() external pure returns (uint256 ptr) {
        assembly {
            ptr := mload(0x40)
            mstore(0x40, ptr)
        }
    }
}
