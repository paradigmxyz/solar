//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=mir,evm-ir-runtime
//@[ir] filecheck:
//@[mir] filecheck: --check-prefix=MIR
//@ run-call: ReturningMemoryEffects::probe [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 1, 2, 10 => 5, 0
//@ run-call: ReturningMemoryEffects::probe [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22], 1, 2, 10 => 5, 1771
//@ run-call: ReturningMemoryEffects::probe [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22], 0, 0, 17 => 17, 1771
//@ run-call: ReturningMemoryEffects::probe [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22], 1, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1771
//@ run-call: ReturningMemoryEffects::probe [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22], 2, 3, 6 => 4, 1771
//@ run-call: ReturningMemoryWriters::normalWriter [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0, 4096 => 0, 1, 1
//@ run-call: ReturningMemoryWriters::normalWriter [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22], 2, 4097 => 1771, 3, 3
//@ run-call: ReturningMemoryWriters::preBranchWriter [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0, 4096, false => 0, 1, 9
//@ run-call: ReturningMemoryWriters::preBranchWriter [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22], 2, 4097, false => 1771, 3, 9
//@ run-call: ReturningMemoryWriters::zeroResultWriter [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0, 4096 => 0, 1
//@ run-call: ReturningMemoryWriters::zeroResultWriter [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22], 2, 4097 => 1771, 3
//@ run-call-fail: ReturningMemoryEffects::probe [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 1, 0, 10 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: ReturningMemoryEffects::probe [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0, 1, 10 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: ReturningMemoryEffects::probe [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 2, 3, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: ReturningMemoryEffects::probe [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 2, 3, 5 => 0xc63cf089
//@ run-call-fail: ReturningMemoryWriters::preBranchWriter [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22], 2, 4096, true => 0x000000000000000000000000000000000000000000000000000000000000000b

// The arithmetic helper writes error data only when it reverts. Its caller
// consumes twenty-two independent values after a normal return. The remaining
// methods retain writes on returning paths, including a zero-result callee.
// MIR-LABEL: fn @probe(
// MIR: icall @_getFraction, 1,
// MIR-LABEL: fn @normalWriter(
// MIR: icall @writeCount, 1,
// MIR-LABEL: fn @preBranchWriter(
// MIR: icall @writeBeforeBranch, 1,
// MIR-LABEL: fn @zeroResultWriter(
// MIR: icall @writeNoResult, 0,
// MIR-LABEL: fn @writeCount(
// MIR: mstore
// MIR-NEXT: ret
// MIR-LABEL: fn @writeBeforeBranch(
// MIR: mstore
// MIR-NEXT: [[IS_ZERO:v[0-9]+]] = eq arg2, 0
// MIR-NEXT: [[REJECT:v[0-9]+]] = iszero [[IS_ZERO]]
// MIR-NEXT: jumpi [[REJECT]]
// MIR: ret
// MIR: revert
// MIR-LABEL: fn @writeNoResult(
// MIR: icall @writeNoResult, 0,
// MIR: mstore
// MIR-NEXT: stop

// The guarded calldata roots belong to probe. After its array-bounds panic
// block, the final two roots end directly in the fraction call's arguments,
// without reloading the initialized homes as caller backups.
// CHECK-LABEL: @module ReturningMemoryEffects_runtime
// CHECK: push 50{{$}}
// CHECK-NEXT: push 4{{$}}
// CHECK-NEXT: mstore
// CHECK-NEXT: push 36{{$}}
// CHECK-NEXT: push 0{{$}}
// CHECK-NEXT: revert
// CHECK: push 644{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 7{{$}}
// CHECK-NEXT: mul
// CHECK-NEXT: push [[PENULTIMATE:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push 1{{$}}
// CHECK-NEXT: jumpi [[LAST_ROOT:bb[0-9]+]], {{bb[0-9]+}}
// CHECK-NEXT: [[LAST_ROOT]]:
// CHECK-NEXT: push 676{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 7{{$}}
// CHECK-NEXT: mul
// CHECK-NEXT: push [[LAST_HOME:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push [[CONTINUATION:bb[0-9]+]]
// CHECK-NEXT: push 772{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 740{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 708{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: swap 2{{$}}
// CHECK-NEXT: dup 3{{$}}
// CHECK-NEXT: dup 3{{$}}
// CHECK-NEXT: eq
// CHECK-NEXT: jumpi {{bb[0-9]+}}, {{bb[0-9]+}}
contract ReturningMemoryEffects {
    error InexactFraction();
    function probe(uint256[22] calldata keep, uint256 n, uint256 d, uint256 value)
        external pure returns (uint256 fraction, uint256 checksum)
    {
        unchecked {
            uint256 a0 = keep[0] * 7;
            uint256 a1 = keep[1] * 7;
            uint256 a2 = keep[2] * 7;
            uint256 a3 = keep[3] * 7;
            uint256 a4 = keep[4] * 7;
            uint256 a5 = keep[5] * 7;
            uint256 a6 = keep[6] * 7;
            uint256 a7 = keep[7] * 7;
            uint256 a8 = keep[8] * 7;
            uint256 a9 = keep[9] * 7;
            uint256 a10 = keep[10] * 7;
            uint256 a11 = keep[11] * 7;
            uint256 a12 = keep[12] * 7;
            uint256 a13 = keep[13] * 7;
            uint256 a14 = keep[14] * 7;
            uint256 a15 = keep[15] * 7;
            uint256 a16 = keep[16] * 7;
            uint256 a17 = keep[17] * 7;
            uint256 a18 = keep[18] * 7;
            uint256 a19 = keep[19] * 7;
            uint256 a20 = keep[20] * 7;
            uint256 a21 = keep[21] * 7;
            fraction = _getFraction(n, d, value);
            checksum = a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12 + a13 + a14 + a15 + a16 + a17 + a18 + a19 + a20 + a21;
        }
    }
    function _getFraction(uint256 numerator, uint256 denominator, uint256 value)
        internal pure returns (uint256 newValue)
    {
        // Return value early in cases where the fraction resolves to 1.
        if (numerator == denominator) {
            return value;
        }

        // Multiply the numerator by the value and ensure no overflow occurs.
        uint256 valueTimesNumerator = value * numerator;

        // Divide that value by the denominator to get the new value.
        newValue = valueTimesNumerator / denominator;

        // Ensure that division gave a final result with no remainder.
        bool exact = ((newValue * denominator) / numerator) == value;
        if (!exact) {
            revert InexactFraction();
        }
    }

}

contract ReturningMemoryWriters {
    function normalWriter(uint256[22] calldata values, uint256 depth, uint256 destination)
        external pure returns (uint256 checksum, uint256 steps, uint256 observed)
    {
        assembly {
            function writeCount(n, ptr) -> result {
                if n { result := writeCount(sub(n, 1), ptr) }
                result := add(result, 1)
                mstore(ptr, result)
            }
            let a00 := mul(calldataload(add(values, 0)), 7)
            let a01 := mul(calldataload(add(values, 32)), 7)
            let a02 := mul(calldataload(add(values, 64)), 7)
            let a03 := mul(calldataload(add(values, 96)), 7)
            let a04 := mul(calldataload(add(values, 128)), 7)
            let a05 := mul(calldataload(add(values, 160)), 7)
            let a06 := mul(calldataload(add(values, 192)), 7)
            let a07 := mul(calldataload(add(values, 224)), 7)
            let a08 := mul(calldataload(add(values, 256)), 7)
            let a09 := mul(calldataload(add(values, 288)), 7)
            let a10 := mul(calldataload(add(values, 320)), 7)
            let a11 := mul(calldataload(add(values, 352)), 7)
            let a12 := mul(calldataload(add(values, 384)), 7)
            let a13 := mul(calldataload(add(values, 416)), 7)
            let a14 := mul(calldataload(add(values, 448)), 7)
            let a15 := mul(calldataload(add(values, 480)), 7)
            let a16 := mul(calldataload(add(values, 512)), 7)
            let a17 := mul(calldataload(add(values, 544)), 7)
            let a18 := mul(calldataload(add(values, 576)), 7)
            let a19 := mul(calldataload(add(values, 608)), 7)
            let a20 := mul(calldataload(add(values, 640)), 7)
            let a21 := mul(calldataload(add(values, 672)), 7)
            steps := writeCount(depth, destination)
            observed := mload(destination)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17), a18), a19), a20), a21)
        }
    }

    function preBranchWriter(uint256[22] calldata values, uint256 depth, uint256 destination, bool fail)
        external pure returns (uint256 checksum, uint256 steps, uint256 observed)
    {
        assembly {
            function writeBeforeBranch(n, ptr, reject) -> result {
                mstore(ptr, add(n, 9))
                if reject { revert(ptr, 32) }
                if n { result := writeBeforeBranch(sub(n, 1), ptr, reject) }
                result := add(result, 1)
            }
            let a00 := mul(calldataload(add(values, 0)), 7)
            let a01 := mul(calldataload(add(values, 32)), 7)
            let a02 := mul(calldataload(add(values, 64)), 7)
            let a03 := mul(calldataload(add(values, 96)), 7)
            let a04 := mul(calldataload(add(values, 128)), 7)
            let a05 := mul(calldataload(add(values, 160)), 7)
            let a06 := mul(calldataload(add(values, 192)), 7)
            let a07 := mul(calldataload(add(values, 224)), 7)
            let a08 := mul(calldataload(add(values, 256)), 7)
            let a09 := mul(calldataload(add(values, 288)), 7)
            let a10 := mul(calldataload(add(values, 320)), 7)
            let a11 := mul(calldataload(add(values, 352)), 7)
            let a12 := mul(calldataload(add(values, 384)), 7)
            let a13 := mul(calldataload(add(values, 416)), 7)
            let a14 := mul(calldataload(add(values, 448)), 7)
            let a15 := mul(calldataload(add(values, 480)), 7)
            let a16 := mul(calldataload(add(values, 512)), 7)
            let a17 := mul(calldataload(add(values, 544)), 7)
            let a18 := mul(calldataload(add(values, 576)), 7)
            let a19 := mul(calldataload(add(values, 608)), 7)
            let a20 := mul(calldataload(add(values, 640)), 7)
            let a21 := mul(calldataload(add(values, 672)), 7)
            steps := writeBeforeBranch(depth, destination, fail)
            observed := mload(destination)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17), a18), a19), a20), a21)
        }
    }

    function zeroResultWriter(uint256[22] calldata values, uint256 depth, uint256 destination)
        external pure returns (uint256 checksum, uint256 observed)
    {
        assembly {
            function writeNoResult(n, ptr) {
                if n { writeNoResult(sub(n, 1), ptr) }
                mstore(ptr, add(n, 1))
            }
            let a00 := mul(calldataload(add(values, 0)), 7)
            let a01 := mul(calldataload(add(values, 32)), 7)
            let a02 := mul(calldataload(add(values, 64)), 7)
            let a03 := mul(calldataload(add(values, 96)), 7)
            let a04 := mul(calldataload(add(values, 128)), 7)
            let a05 := mul(calldataload(add(values, 160)), 7)
            let a06 := mul(calldataload(add(values, 192)), 7)
            let a07 := mul(calldataload(add(values, 224)), 7)
            let a08 := mul(calldataload(add(values, 256)), 7)
            let a09 := mul(calldataload(add(values, 288)), 7)
            let a10 := mul(calldataload(add(values, 320)), 7)
            let a11 := mul(calldataload(add(values, 352)), 7)
            let a12 := mul(calldataload(add(values, 384)), 7)
            let a13 := mul(calldataload(add(values, 416)), 7)
            let a14 := mul(calldataload(add(values, 448)), 7)
            let a15 := mul(calldataload(add(values, 480)), 7)
            let a16 := mul(calldataload(add(values, 512)), 7)
            let a17 := mul(calldataload(add(values, 544)), 7)
            let a18 := mul(calldataload(add(values, 576)), 7)
            let a19 := mul(calldataload(add(values, 608)), 7)
            let a20 := mul(calldataload(add(values, 640)), 7)
            let a21 := mul(calldataload(add(values, 672)), 7)
            writeNoResult(depth, destination)
            observed := mload(destination)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17), a18), a19), a20), a21)
        }
    }
}
