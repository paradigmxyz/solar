//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[mir] filecheck: --check-prefix=MIR
//@ run-call: probe 1, 0 => 0, 0x0000000000000000000000000000000000001234
//@ run-call: probe 0x0100000000000000000000000000000000000000000000000000000000000001, 2 => 0x0100000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000001234
//@ run-call-fail: probe 0, 0 => 0x

// The recursive validator has thirteen live parameters and needs a framed
// activation. Its source write remains conservative; zero fails validation.
// A separate clean count call precedes the storage-address mask. Removing its
// protection must pay for any lost compact-literal construction in this block.
// MIR-LABEL: @module CallRegionLiteralBudget
// MIR-LABEL: fn @probe(
// MIR: icall @validate, 1,
// MIR: icall @count, 1,
// MIR-LABEL: fn @validate(
// MIR: icall @validate, 1,
// MIR-LABEL: fn @count(
// MIR: icall @count, 1,
// CHECK-LABEL: @module CallRegionLiteralBudget_runtime
// CHECK: sload
// CHECK-NEXT: push 0{{$}}
// CHECK-NEXT: not
// CHECK-NEXT: push 96{{$}}
// CHECK-NEXT: shr
// CHECK-NEXT: and
contract CallRegionLiteralBudget {
    address private saved = address(0x1234);

    function probe(uint256 word, uint256 depth) external view returns (uint256 high, address low) {
        assembly {
            function validate(n, a, b, c, d, e, f, g, h, i, j, k, l) -> result {
                result := add(add(add(add(add(add(add(add(add(add(add(a, b), c), d), e), f), g), h), i), j), k), l)
                if eq(result, 66) { revert(0, 0) }
                mstore(0, result)
                if n { result := add(result, validate(sub(n, 1), a, b, c, d, e, f, g, h, i, j, k, l)) }
            }
            function count(n) -> result {
                if n { result := count(sub(n, 1)) }
                result := add(result, 1)
            }
            pop(validate(depth, word, add(word, 1), add(word, 2), add(word, 3), add(word, 4), add(word, 5), add(word, 6), add(word, 7), add(word, 8), add(word, 9), add(word, 10), add(word, 11)))
            high := and(word, shl(248, 255))
            pop(count(depth))
            low := and(sload(0), 0xffffffffffffffffffffffffffffffffffffffff)
        }
    }
}
