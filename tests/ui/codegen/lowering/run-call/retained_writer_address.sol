//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: FrozenWriterBase::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32], 4064, 4096 => 99, 34320
//@ run-call: RetainedWriterAddress::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4064, 4096 => 53, 513
//@ run-call: RetainedWriterAddress::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4065, 4097 => 54, 513
//@ run-call: RetainedWriterAddress::run [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 4064, 4096 => 0, 0
//@ run-call: RepeatedWriterAddress::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4064, 4096 => 4096, 566
//@ run-call: InterveningByteWriter::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4064, 4096 => 53, 520
//@ run-call: ResidentWriterSuffix::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4064, 4096 => 53, 513

// Eighteen values cross bounce and retain their mandatory homes. The fresh
// address copy pays for its removed reload without changing the protected writer.
// CHECK-LABEL: @module RetainedWriterAddress_runtime
// CHECK: push 32{{$}}
// CHECK-NEXT: push 580{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: add
// CHECK-NEXT: dup 1
// CHECK-NEXT: push [[ADDR:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push [[VALUE:[0-9]+]]
// CHECK-NEXT: mload
// CHECK-NEXT: swap 1
// CHECK-NEXT: push 192{{$}}
// CHECK-NEXT: dup 2
// CHECK-NEXT: push 31{{$}}
// CHECK: mstore
// CHECK-NEXT: swap 1
// CHECK-NEXT: mstore
// CHECK-NEXT: mstore
// CHECK: push 64{{$}}
// CHECK-NEXT: push 128{{$}}
// CHECK-NEXT: return
contract RetainedWriterAddress {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum)
    {
        assembly {
            function bounce(x, depth) -> result {
                result := add(x, depth)
                if depth { result := bounce(result, sub(depth, 1)) }
            }
            let a00 := mul(calldataload(add(values, 0)), 3)
            let a01 := mul(calldataload(add(values, 32)), 3)
            let a02 := mul(calldataload(add(values, 64)), 3)
            let a03 := mul(calldataload(add(values, 96)), 3)
            let a04 := mul(calldataload(add(values, 128)), 3)
            let a05 := mul(calldataload(add(values, 160)), 3)
            let a06 := mul(calldataload(add(values, 192)), 3)
            let a07 := mul(calldataload(add(values, 224)), 3)
            let a08 := mul(calldataload(add(values, 256)), 3)
            let a09 := mul(calldataload(add(values, 288)), 3)
            let a10 := mul(calldataload(add(values, 320)), 3)
            let a11 := mul(calldataload(add(values, 352)), 3)
            let a12 := mul(calldataload(add(values, 384)), 3)
            let a13 := mul(calldataload(add(values, 416)), 3)
            let a14 := mul(calldataload(add(values, 448)), 3)
            let a15 := mul(calldataload(add(values, 480)), 3)
            let a16 := mul(calldataload(add(values, 512)), 3)
            let a17 := mul(calldataload(add(values, 544)), 3)
            let payload := bounce(xor(a00, a17), and(destination, 1))
            let ptr := add(destination, 32)
            mstore(ptr, payload)
            observed := mload(source)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}

// Reusing the address as both operands follows the existing adjacent reload
// peephole. It does not use the new distinct-address-operand path.
// CHECK-LABEL: @module RepeatedWriterAddress_runtime
// CHECK: push 32{{$}}
// CHECK-NEXT: push 580{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: add
// CHECK-NEXT: dup 1
// CHECK-NEXT: push [[REPEATED:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 192{{$}}
contract RepeatedWriterAddress {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum)
    {
        assembly {
            function bounce(x, depth) -> result {
                result := add(x, depth)
                if depth { result := bounce(result, sub(depth, 1)) }
            }
            let a00 := mul(calldataload(add(values, 0)), 3)
            let a01 := mul(calldataload(add(values, 32)), 3)
            let a02 := mul(calldataload(add(values, 64)), 3)
            let a03 := mul(calldataload(add(values, 96)), 3)
            let a04 := mul(calldataload(add(values, 128)), 3)
            let a05 := mul(calldataload(add(values, 160)), 3)
            let a06 := mul(calldataload(add(values, 192)), 3)
            let a07 := mul(calldataload(add(values, 224)), 3)
            let a08 := mul(calldataload(add(values, 256)), 3)
            let a09 := mul(calldataload(add(values, 288)), 3)
            let a10 := mul(calldataload(add(values, 320)), 3)
            let a11 := mul(calldataload(add(values, 352)), 3)
            let a12 := mul(calldataload(add(values, 384)), 3)
            let a13 := mul(calldataload(add(values, 416)), 3)
            let a14 := mul(calldataload(add(values, 448)), 3)
            let a15 := mul(calldataload(add(values, 480)), 3)
            let a16 := mul(calldataload(add(values, 512)), 3)
            let a17 := mul(calldataload(add(values, 544)), 3)
            let payload := bounce(xor(a00, a17), and(destination, 1))
            let ptr := add(destination, 32)
            mstore(ptr, ptr)
            observed := mload(source)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17), payload)
        }
    }
}

// The byte writer separates the address producer from its MSTORE consumer.
// CHECK-LABEL: @module InterveningByteWriter_runtime
// CHECK: push 32{{$}}
// CHECK-NEXT: push 580{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: add
// CHECK-NEXT: push [[SEPARATED:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push 7{{$}}
// CHECK-NEXT: push 0{{$}}
// CHECK-NEXT: mstore8
// CHECK-NEXT: push [[PAYLOAD:[0-9]+]]
// CHECK-NEXT: mload
// CHECK-NEXT: push [[SEPARATED]]
// CHECK-NEXT: mload
// CHECK-NEXT: push 192{{$}}
contract InterveningByteWriter {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum)
    {
        assembly {
            function bounce(x, depth) -> result {
                result := add(x, depth)
                if depth { result := bounce(result, sub(depth, 1)) }
            }
            let a00 := mul(calldataload(add(values, 0)), 3)
            let a01 := mul(calldataload(add(values, 32)), 3)
            let a02 := mul(calldataload(add(values, 64)), 3)
            let a03 := mul(calldataload(add(values, 96)), 3)
            let a04 := mul(calldataload(add(values, 128)), 3)
            let a05 := mul(calldataload(add(values, 160)), 3)
            let a06 := mul(calldataload(add(values, 192)), 3)
            let a07 := mul(calldataload(add(values, 224)), 3)
            let a08 := mul(calldataload(add(values, 256)), 3)
            let a09 := mul(calldataload(add(values, 288)), 3)
            let a10 := mul(calldataload(add(values, 320)), 3)
            let a11 := mul(calldataload(add(values, 352)), 3)
            let a12 := mul(calldataload(add(values, 384)), 3)
            let a13 := mul(calldataload(add(values, 416)), 3)
            let a14 := mul(calldataload(add(values, 448)), 3)
            let a15 := mul(calldataload(add(values, 480)), 3)
            let a16 := mul(calldataload(add(values, 512)), 3)
            let a17 := mul(calldataload(add(values, 544)), 3)
            let payload := bounce(xor(a00, a17), and(destination, 1))
            let ptr := add(destination, 32)
            mstore8(0, 7)
            mstore(ptr, payload)
            observed := mload(source)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17), byte(0, mload(0)))
        }
    }
}

// Without the call boundary, the live bank retains a physical suffix and only
// two live spill homes need ordinary backup. The address is stored and reloaded.
// CHECK-LABEL: @module ResidentWriterSuffix_runtime
// CHECK: push 32{{$}}
// CHECK-NEXT: push 580{{$}}
// CHECK-NEXT: calldataload
// CHECK-NEXT: add
// CHECK-NEXT: push [[RESIDENT_ADDR:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push 192{{$}}
// CHECK-NEXT: mload
// CHECK-NEXT: push 224{{$}}
// CHECK-NEXT: mload
// CHECK-NEXT: push [[RESIDENT_PAYLOAD:[0-9]+]]
// CHECK-NEXT: mload
// CHECK-NEXT: push [[RESIDENT_ADDR]]
// CHECK-NEXT: mload
// CHECK-NEXT: mstore
contract ResidentWriterSuffix {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum)
    {
        assembly {
            let a00 := mul(calldataload(add(values, 0)), 3)
            let a01 := mul(calldataload(add(values, 32)), 3)
            let a02 := mul(calldataload(add(values, 64)), 3)
            let a03 := mul(calldataload(add(values, 96)), 3)
            let a04 := mul(calldataload(add(values, 128)), 3)
            let a05 := mul(calldataload(add(values, 160)), 3)
            let a06 := mul(calldataload(add(values, 192)), 3)
            let a07 := mul(calldataload(add(values, 224)), 3)
            let a08 := mul(calldataload(add(values, 256)), 3)
            let a09 := mul(calldataload(add(values, 288)), 3)
            let a10 := mul(calldataload(add(values, 320)), 3)
            let a11 := mul(calldataload(add(values, 352)), 3)
            let a12 := mul(calldataload(add(values, 384)), 3)
            let a13 := mul(calldataload(add(values, 416)), 3)
            let a14 := mul(calldataload(add(values, 448)), 3)
            let a15 := mul(calldataload(add(values, 480)), 3)
            let a16 := mul(calldataload(add(values, 512)), 3)
            let a17 := mul(calldataload(add(values, 544)), 3)
            let payload := xor(a00, a17)
            let ptr := add(destination, 32)
            mstore(ptr, payload)
            observed := mload(source)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}

// A large no-call bank keeps a frozen resident base below the two operands.
// CHECK-LABEL: @module FrozenWriterBase_runtime
// CHECK: push 32{{$}}
// CHECK-NEXT: push 0x404
// CHECK-NEXT: calldataload
// CHECK-NEXT: add
// CHECK-NEXT: dup 1
// CHECK-NEXT: push [[FROZEN_ADDR:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push [[FROZEN_VALUE:[0-9]+]]
// CHECK-NEXT: mload
// CHECK-NEXT: swap 1
// CHECK-NEXT: push 192{{$}}
// CHECK-NEXT: dup 2
// CHECK-NEXT: push 31{{$}}
// CHECK: mstore
// CHECK-NEXT: swap 1
// CHECK-NEXT: mstore
// CHECK-NEXT: mstore
// CHECK: push 64{{$}}
// CHECK-NEXT: push 0{{$}}
// CHECK-NEXT: return
contract FrozenWriterBase {
    function run(uint256[32] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum)
    {
        assembly {
            let a00 := mul(calldataload(add(values, 0)), 3)
            let a01 := mul(calldataload(add(values, 32)), 3)
            let a02 := mul(calldataload(add(values, 64)), 3)
            let a03 := mul(calldataload(add(values, 96)), 3)
            let a04 := mul(calldataload(add(values, 128)), 3)
            let a05 := mul(calldataload(add(values, 160)), 3)
            let a06 := mul(calldataload(add(values, 192)), 3)
            let a07 := mul(calldataload(add(values, 224)), 3)
            let a08 := mul(calldataload(add(values, 256)), 3)
            let a09 := mul(calldataload(add(values, 288)), 3)
            let a10 := mul(calldataload(add(values, 320)), 3)
            let a11 := mul(calldataload(add(values, 352)), 3)
            let a12 := mul(calldataload(add(values, 384)), 3)
            let a13 := mul(calldataload(add(values, 416)), 3)
            let a14 := mul(calldataload(add(values, 448)), 3)
            let a15 := mul(calldataload(add(values, 480)), 3)
            let a16 := mul(calldataload(add(values, 512)), 3)
            let a17 := mul(calldataload(add(values, 544)), 3)
            let a18 := mul(calldataload(add(values, 576)), 3)
            let a19 := mul(calldataload(add(values, 608)), 3)
            let a20 := mul(calldataload(add(values, 640)), 3)
            let a21 := mul(calldataload(add(values, 672)), 3)
            let a22 := mul(calldataload(add(values, 704)), 3)
            let a23 := mul(calldataload(add(values, 736)), 3)
            let a24 := mul(calldataload(add(values, 768)), 3)
            let a25 := mul(calldataload(add(values, 800)), 3)
            let a26 := mul(calldataload(add(values, 832)), 3)
            let a27 := mul(calldataload(add(values, 864)), 3)
            let a28 := mul(calldataload(add(values, 896)), 3)
            let a29 := mul(calldataload(add(values, 928)), 3)
            let a30 := mul(calldataload(add(values, 960)), 3)
            let a31 := mul(calldataload(add(values, 992)), 3)
            let payload := xor(a00, a31)
            let ptr := add(destination, 32)
            mstore(ptr, payload)
            observed := mload(source)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, mul(a01, 2)), mul(a02, 3)), mul(a03, 4)), mul(a04, 5)), mul(a05, 6)), mul(a06, 7)), mul(a07, 8)), mul(a08, 9)), mul(a09, 10)), mul(a10, 11)), mul(a11, 12)), mul(a12, 13)), mul(a13, 14)), mul(a14, 15)), mul(a15, 16)), mul(a16, 17)), mul(a17, 18)), mul(a18, 19)), mul(a19, 20)), mul(a20, 21)), mul(a21, 22)), mul(a22, 23)), mul(a23, 24)), mul(a24, 25)), mul(a25, 26)), mul(a26, 27)), mul(a27, 28)), mul(a28, 29)), mul(a29, 30)), mul(a30, 31)), mul(a31, 32))
        }
    }
}
