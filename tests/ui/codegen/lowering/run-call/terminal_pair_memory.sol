//@ codegen-matrix: standard ir legacy
//@ compile-flags: -Zevm-ir-pipeline=terminal-prefixes
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[legacy] compile-flags: -Ogas --evm-version paris
//@ run-call: forward 11, 22 => 11, 22
//@ run-call: forward 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: reverse 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
//@ run-call: priorRead 7, 9 => 7, 9
//@ run-call: observed 7 => 7, 160
//@ run-call: aliased 7 => 7, 7
//@ run-call: duplicate => 22, 0
//@ run-call-fail: hugeAddress; gas=100000

// CHECK-LABEL: @module TerminalPairMemory_runtime
// CHECK: push 64
// CHECK-NEXT: push 0
// CHECK-NEXT: return
contract TerminalPairMemory {
    function forward(uint256 a, uint256 b) external pure returns (uint256, uint256) {
        assembly {
            mstore(128, a)
            mstore(160, b)
            return(128, 64)
        }
    }

    function reverse(uint256 a, uint256 b) external pure returns (uint256, uint256) {
        assembly {
            mstore(160, b)
            mstore(128, a)
            return(128, 64)
        }
    }

    function priorRead(uint256 a, uint256 b) external pure returns (uint256, uint256) {
        assembly {
            mstore(0, a)
            let value := mload(0)
            mstore(128, value)
            mstore(160, b)
            return(128, 64)
        }
    }

    function observed(uint256 a) external pure returns (uint256, uint256) {
        assembly {
            mstore(128, a)
            mstore(160, msize())
            return(128, 64)
        }
    }

    function aliased(uint256 a) external pure returns (uint256, uint256) {
        assembly {
            mstore(128, a)
            mstore(160, mload(128))
            return(128, 64)
        }
    }

    function duplicate() external pure returns (uint256, uint256) {
        assembly {
            mstore(128, 11)
            mstore(128, 22)
            return(128, 64)
        }
    }

    function hugeAddress() external pure {
        assembly {
            mstore(0x100000000, 11)
            mstore(0x100000020, 22)
            return(0x100000000, 64)
        }
    }
}
