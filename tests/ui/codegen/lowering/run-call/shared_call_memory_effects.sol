//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: CallSummaryClean::run [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0 => 0, 1
//@ run-call: CallSummaryClean::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 2 => 1197, 3
//@ run-call: CallSummaryUnknownWriter::run [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0, 4096 => 0, 1, 1
//@ run-call: CallSummaryUnknownWriter::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 2, 4097 => 1197, 3, 3
//@ run-call: CallSummaryMultipleResults::run [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0 => 0, 1, 3, 2, 5
//@ run-call: CallSummaryMultipleResults::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 2 => 1197, 3, 7, 4, 9

// A one-result recursive arithmetic call has no source writes or additional-result buffer.
// All eighteen independently computed values are consumed after the call.
contract CallSummaryClean {
    function run(uint256[18] calldata values, uint256 depth)
        external pure returns (uint256 checksum, uint256 steps)
    {
        assembly {
            function count(n) -> result {
                if n { result := count(sub(n, 1)) }
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
            steps := count(depth)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}

// A calldata-selected destination keeps the callee memory write conservative.
// Its final write and the caller checksum must both survive the call.
// CHECK-LABEL: @module CallSummaryUnknownWriter_runtime
// CHECK: push 4{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 7{{$}}
// CHECK-NEXT: mul
// CHECK-NEXT: push [[WFIRST:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push 36{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 7{{$}}
// CHECK-NEXT: mul
// CHECK-NEXT: push [[WSECOND:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK: push 548{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 7{{$}}
// CHECK-NEXT: mul
// CHECK-NEXT: push [[WLAST:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push [[WFIRST]]
// CHECK-NEXT: mload
// CHECK-NEXT: push [[WSECOND]]
// CHECK-NEXT: mload
// CHECK: push [[WRETURN:bb[0-9]+]]
// CHECK-NEXT: push 612{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 580{{$}}
// CHECK-NEXT: calldataload
// CHECK: [[WRETURN]]:
// CHECK-NEXT: swap 16
// CHECK-NEXT: push {{[0-9]+}}
// CHECK-NEXT: mstore
// CHECK-NEXT: push [[WLAST]]
// CHECK-NEXT: mstore
contract CallSummaryUnknownWriter {
    function run(uint256[18] calldata values, uint256 depth, uint256 destination)
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
            steps := writeCount(depth, destination)
            observed := mload(destination)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}

// Arithmetic purity does not remove the hidden buffer for additional results.
// The second call must not overwrite the first pair or the live checksum inputs.
// CHECK-LABEL: @module CallSummaryMultipleResults_runtime
// CHECK: push 4{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 7{{$}}
// CHECK-NEXT: mul
// CHECK-NEXT: push [[PFIRST:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push 36{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 7{{$}}
// CHECK-NEXT: mul
// CHECK-NEXT: push [[PSECOND:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK: push 548{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 7{{$}}
// CHECK-NEXT: mul
// CHECK-NEXT: push [[PLAST:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push [[PFIRST]]
// CHECK-NEXT: mload
// CHECK-NEXT: push [[PSECOND]]
// CHECK-NEXT: mload
// CHECK: push [[PRETURN:bb[0-9]+]]
// CHECK-NEXT: push 580{{$}}
// CHECK-NEXT: calldataload
// CHECK: [[PRETURN]]:
// CHECK-NEXT: swap 16
// CHECK-NEXT: push {{[0-9]+}}
// CHECK-NEXT: mstore
// CHECK-NEXT: push [[PLAST]]
// CHECK-NEXT: mstore
// CHECK: push 32{{$}}
// CHECK-NEXT: mload
// CHECK-NEXT: push 32{{$}}
// CHECK-NEXT: add
// CHECK-NEXT: mload
contract CallSummaryMultipleResults {
    function run(uint256[18] calldata values, uint256 depth)
        external pure returns (uint256 checksum, uint256 first, uint256 second, uint256 nextFirst, uint256 nextSecond)
    {
        assembly {
            function pair(n) -> x, y {
                switch n
                case 0 { x := 1 y := 3 }
                default {
                    x, y := pair(sub(n, 1))
                    x := add(x, 1)
                    y := add(y, 2)
                }
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
            first, second := pair(depth)
            nextFirst, nextSecond := pair(add(depth, 1))
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}
