//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract MirAllocOps {
    // CHECK-LABEL: fn @fixedArray{{[( ]}}
    // CHECK: = alloc memoryfixedarray<2, 1>, exact, uninitialized, infallible, 64
    function fixedArray(uint256 value) external pure returns (uint256) {
        uint256[2] memory words;
        words[0] = value;
        return words[0];
    }

    // CHECK-LABEL: fn @dynamic{{[( ]}}
    // CHECK: = alloc memorybytes, exact, uninitialized, infallible,
    function dynamic(bytes calldata data) external pure returns (bytes memory) {
        return data;
    }

    // CHECK-LABEL: fn @frameShadow{{[( ]}}
    // CHECK: ret 1, 2, 3, 4
    function frameShadow()
        external
        pure
        returns (uint256, uint256, uint256, uint256)
    {
        return (1, 2, 3, 4);
    }

    // CHECK-LABEL: fn @rawAssembly{{[( ]}}
    // CHECK: = mload 64
    // CHECK: mstore 64,
    function rawAssembly() external pure returns (uint256 ptr) {
        assembly {
            ptr := mload(0x40)
            mstore(0x40, ptr)
        }
    }
}
