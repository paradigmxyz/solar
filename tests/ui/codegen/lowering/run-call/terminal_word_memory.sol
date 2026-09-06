//@ codegen-matrix: standard ir
//@ compile-flags: -Zevm-ir-pipeline=terminal-prefixes
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: word32 0 => 0
//@ run-call: word32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: word128 42 => 42
//@ run-call: aliasRead => 7
//@ run-call: memoryObserved => 160
//@ run-call: mismatchedRange => 0
//@ run-call: gasPrefix => true
//@ run-call: gasBetween => 7, true
//@ run-call-fail: revertWord => 0x0000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: hugeAddress; gas=100000

// CHECK-LABEL: @module TerminalWordMemory_runtime
// CHECK: calldataload
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: return
contract TerminalWordMemory {
    function word32(uint256 value) external pure returns (uint256) {
        assembly {
            mstore(32, value)
            return(32, 32)
        }
    }

    function word128(uint256 value) external pure returns (uint256) {
        assembly {
            mstore(128, value)
            return(128, 32)
        }
    }

    function aliasRead() external pure returns (uint256) {
        assembly {
            mstore(0, 7)
            mstore(128, mload(0))
            return(128, 32)
        }
    }

    function memoryObserved() external pure returns (uint256) {
        assembly {
            mstore(128, 7)
            let observed := msize()
            mstore(128, observed)
            return(128, 32)
        }
    }

    function mismatchedRange() external pure returns (uint256) {
        assembly {
            mstore(128, 7)
            return(0, 32)
        }
    }

    function gasPrefix() external view returns (bool) {
        assembly {
            mstore(128, gt(gas(), 0))
            return(128, 32)
        }
    }

    function gasBetween() external view returns (uint256, bool) {
        assembly {
            mstore(128, 7)
            mstore(160, gt(gas(), 0))
            return(128, 64)
        }
    }

    function revertWord() external pure {
        assembly {
            mstore(128, 7)
            revert(128, 32)
        }
    }

    function hugeAddress() external pure {
        assembly {
            mstore(0x100000000, 7)
            return(0x100000000, 32)
        }
    }
}
