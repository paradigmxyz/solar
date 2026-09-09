//@ codegen-matrix: standard ir
//@[ir] filecheck:
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ run-call: dying => 1
//@ run-call: live => 17
//@ run-call: repeated => 17
//@ run-call: retained => 51
//@ run-call: interleaved => 1
//@ run-call: nonliteral 123 => 1
//@ run-call: repeatedZero => 17
//@ run-call: repeatedWide => 17

// Resident preparation is checked in emitted selector order.
// CHECK-LABEL: @module ResidentBeforeLiterals_runtime

// repeatedWide.
// CHECK: push 0xae1ec012
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[BODY0:bb[0-9]+]]
// CHECK-NEXT: [[BODY0]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: sload
// CHECK-NEXT: push 153
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 0x123456789abcdef0123456789abcdef0fedcba98765432100f1e2d3c4b5a6978
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: log3

// retained.
// CHECK: push 0x5216948c
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[BODY1:bb[0-9]+]]
// CHECK-NEXT: [[BODY1]]:
// CHECK-NEXT: push 2
// CHECK-NEXT: sload
// CHECK-NEXT: push 0
// CHECK-NEXT: sload
// CHECK-NEXT: push 1
// CHECK-NEXT: sload
// CHECK-NEXT: push 153
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: swap 1
// CHECK-NEXT: push 170
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: log3

// dying.
// CHECK: push 0xfbc7852e
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[BODY2:bb[0-9]+]]
// CHECK-NEXT: [[BODY2]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: sload
// CHECK-NEXT: push 1
// CHECK-NEXT: sload
// CHECK-NEXT: push 153
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: swap 1
// CHECK-NEXT: push 170
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: log3

// live.
// CHECK: push 0x957aa58c
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[BODY3:bb[0-9]+]]
// CHECK-NEXT: [[BODY3]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: sload
// CHECK-NEXT: push 153
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 170
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: log2

// repeatedZero.
// CHECK: push 0x2be81da
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[BODY4:bb[0-9]+]]
// CHECK-NEXT: [[BODY4]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: sload
// CHECK-NEXT: push 153
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: dup 1
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 0
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: log3

// interleaved.
// CHECK: push 0xb977aed8
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[BODY5:bb[0-9]+]]
// CHECK-NEXT: [[BODY5]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: sload
// CHECK-NEXT: push 1
// CHECK-NEXT: sload
// CHECK-NEXT: push 153
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 170
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: swap 3
// CHECK-NEXT: swap 4
// CHECK-NEXT: swap 2
// CHECK-NEXT: swap 3
// CHECK-NEXT: log3

// The explicit read is already resident; this is an interleaved operand control.
// CHECK: push 0x6d33b09
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[BODY6:bb[0-9]+]]
// CHECK-NEXT: [[BODY6]]:
// CHECK-NEXT: calldatasize
// CHECK-NEXT: push 36
// CHECK-NEXT: gt
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[WORK6:bb[0-9]+]]
// CHECK-NEXT: [[WORK6]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: sload
// CHECK-NEXT: push 153
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 170
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: exchange 2, 3
// CHECK-NEXT: log3

// repeated.
// CHECK: push 0xc7642fc4
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[BODY7:bb[0-9]+]]
// CHECK-NEXT: [[BODY7]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: sload
// CHECK-NEXT: push 153
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: dup 1
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 170
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: log3
// Storage reads force distinct evaluated values before the event boundary.
contract ResidentBeforeLiterals {
    uint256 private first = 17;
    uint256 private second = 34;
    uint256 private carry = 51;
    uint256 private saved;

    function dying() external returns (uint256) {
        assembly {
            let x := sload(0)
            let y := sload(1)
            mstore(0, 153)
            log3(0, 32, 170, x, y)
        }
        return 1;
    }

    function live() external returns (uint256) {
        assembly {
            let x := sload(0)
            mstore(0, 153)
            log2(0, 32, 170, x)
            sstore(3, x)
        }
        return saved;
    }

    function repeated() external returns (uint256) {
        assembly {
            let x := sload(0)
            mstore(0, 153)
            log3(0, 32, 170, x, x)
            sstore(3, x)
        }
        return saved;
    }

    function retained() external returns (uint256 result) {
        assembly {
            let keep := sload(2)
            let x := sload(0)
            let y := sload(1)
            mstore(0, 153)
            log3(0, 32, 170, x, y)
            result := keep
        }
    }

    function interleaved() external returns (uint256) {
        assembly {
            let x := sload(0)
            let y := sload(1)
            mstore(0, 153)
            log3(0, 32, x, 170, y)
        }
        return 1;
    }

    function nonliteral(uint256) external returns (uint256) {
        assembly {
            let x := sload(0)
            mstore(0, 153)
            log3(0, 32, calldataload(4), 170, x)
        }
        return 1;
    }

    function repeatedZero() external returns (uint256) {
        assembly {
            let x := sload(0)
            mstore(0, 153)
            log3(0, 32, 0, x, x)
            sstore(3, x)
        }
        return saved;
    }

    function repeatedWide() external returns (uint256) {
        assembly {
            let x := sload(0)
            mstore(0, 153)
            log3(0, 32, 0x123456789abcdef0123456789abcdef0fedcba98765432100f1e2d3c4b5a6978, 0x123456789abcdef0123456789abcdef0fedcba98765432100f1e2d3c4b5a6978, x)
            sstore(3, x)
        }
        return saved;
    }
}
