//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: overwritten() => 7, 9
//@ run-call: partialOverwrite() => 7, 0
//@ run-call: expansion() => 320
//@ run-call-fail: returndataBounds()
//@ run-call: allocated 7, 9 => [7, 9]

contract OverwrittenRanges {
    function overwritten() external pure returns (uint256, uint256) {
        assembly {
            calldatacopy(0x101, 0, 64)
            mstore(0x101, 7)
            mstore(0x121, 9)
            return(0x101, 64)
        }
    }

    function partialOverwrite() external pure returns (uint256, uint256) {
        assembly {
            calldatacopy(0x100, calldatasize(), 64)
            mstore(0x100, 7)
            return(0x100, 64)
        }
    }

    function expansion() external pure returns (uint256 size) {
        assembly {
            calldatacopy(0x100, calldatasize(), 64)
            mstore(0x100, 7)
            size := msize() //~[gas,size,dump] WARN: `msize` is not exact when optimizing
            mstore(0x120, 9)
        }
    }

    function returndataBounds() external pure returns (uint256, uint256) {
        assembly {
            returndatacopy(0x100, 0, 64)
            mstore(0x100, 7)
            mstore(0x120, 9)
            return(0x100, 64)
        }
    }

    // CHECK-LABEL: fn @allocated
    // CHECK-NOT: calldatacopy
    // CHECK: }
    function allocated(uint256 x, uint256 y) external pure returns (uint256[] memory values) {
        values = new uint256[](2);
        values[0] = x;
        values[1] = y;
    }
}
