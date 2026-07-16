//@compile-flags: -Zcodegen --emit=mir
//@filecheck: --check-prefix=ALLOC

contract MirAllocOps {
    function fixedArray(uint256 value) external pure returns (uint256) {
        uint256[2] memory words;
        words[0] = value;
        return words[0];
    }

    function dynamic(bytes calldata data) external pure returns (bytes memory) {
        return data;
    }

    function frameShadow()
        external
        pure
        returns (uint256, uint256, uint256, uint256)
    {
        return (1, 2, 3, 4);
    }

    function rawAssembly() external pure returns (uint256 ptr) {
        assembly {
            ptr := mload(0x40)
            mstore(0x40, ptr)
        }
    }
}

// ALLOC-LABEL: fn @fixedArray
// ALLOC: = alloc memoryfixedarray<2, 1>, exact, uninitialized, infallible, 64
// ALLOC-LABEL: fn @dynamic
// ALLOC: = alloc memorybytes, exact, uninitialized, infallible,
// ALLOC-LABEL: fn @rawAssembly
// ALLOC: = mload 64
// ALLOC: mstore 64,
