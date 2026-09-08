//@ codegen-matrix: standard ir legacy
//@ compile-flags: -Zevm-ir-pipeline=terminal-prefixes
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[legacy] compile-flags: -Ogas --evm-version paris
//@ run-call: forward3 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 11 => 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 11
//@ run-call: reverse3 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 22 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 22
//@ run-call: forward4 11, 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 44 => 11, 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 44
//@ run-call: reverse4 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 22, 0, 44 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 22, 0, 44
//@ run-call: observed 7 => 7, 160, 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: aliased 7, 9 => 7, 7, 9

// CHECK-LABEL: @module TerminalMultiMemory_runtime
// CHECK: push 96
// CHECK-NEXT: push 0
// CHECK-NEXT: return
// CHECK: push 128
// CHECK-NEXT: push 0
// CHECK-NEXT: return
contract TerminalMultiMemory {
    function forward3(uint256 a, uint256 b, uint256 c) external pure returns (uint256, uint256, uint256) {
        assembly {
            mstore(128, a)
            mstore(160, b)
            mstore(192, c)
            return(128, 96)
        }
    }

    function reverse3(uint256 a, uint256 b, uint256 c) external pure returns (uint256, uint256, uint256) {
        assembly {
            mstore(192, c)
            mstore(160, b)
            mstore(128, a)
            return(128, 96)
        }
    }

    function forward4(uint256 a, uint256 b, uint256 c, uint256 d)
        external
        pure
        returns (uint256, uint256, uint256, uint256)
    {
        assembly {
            mstore(128, a)
            mstore(160, b)
            mstore(192, c)
            mstore(224, d)
            return(128, 128)
        }
    }

    function reverse4(uint256 a, uint256 b, uint256 c, uint256 d)
        external
        pure
        returns (uint256, uint256, uint256, uint256)
    {
        assembly {
            mstore(224, d)
            mstore(192, c)
            mstore(160, b)
            mstore(128, a)
            return(128, 128)
        }
    }

    // MSIZE is sampled immediately after the original first store.
    function observed(uint256 a) external pure returns (uint256, uint256, uint256, uint256) {
        assembly {
            mstore(128, a)
            mstore(160, msize())
            mstore(192, 0)
            mstore(224, not(0))
            return(128, 128)
        }
    }

    // The second result observes the first word at its original address.
    function aliased(uint256 a, uint256 b) external pure returns (uint256, uint256, uint256) {
        assembly {
            mstore(128, a)
            mstore(160, mload(128))
            mstore(192, b)
            return(128, 96)
        }
    }
}
