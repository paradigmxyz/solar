//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 0 => 210
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 32 => 210

contract FutureHomeWriter {
    function run(uint256[20] calldata values, uint256 distance) external pure returns(uint256 result) {
        assembly {
            // Only the already-live calldata base needs restoring after the write. The twenty
            // loaded values are defined later, so their reusable homes need no backup yet.
            // CHECK: push 0xdeadbeef
            // CHECK-NEXT: push [[TARGET:[0-9a-fx]+]]
            // CHECK-NEXT: mload
            // CHECK-NEXT: mstore
            // CHECK-NEXT: push [[BASE:[0-9a-fx]+]]
            // CHECK-NEXT: mstore
            // CHECK-NEXT: push [[BASE]]
            // CHECK-NEXT: mload
            // CHECK-NEXT: calldataload
            function write(base, delta) -> selected {
                mstore(sub(mload(0x40), delta), 0xdeadbeef)
                let a00 := calldataload(add(base, 0))
                let a01 := calldataload(add(base, 32))
                let a02 := calldataload(add(base, 64))
                let a03 := calldataload(add(base, 96))
                let a04 := calldataload(add(base, 128))
                let a05 := calldataload(add(base, 160))
                let a06 := calldataload(add(base, 192))
                let a07 := calldataload(add(base, 224))
                let a08 := calldataload(add(base, 256))
                let a09 := calldataload(add(base, 288))
                let a10 := calldataload(add(base, 320))
                let a11 := calldataload(add(base, 352))
                let a12 := calldataload(add(base, 384))
                let a13 := calldataload(add(base, 416))
                let a14 := calldataload(add(base, 448))
                let a15 := calldataload(add(base, 480))
                let a16 := calldataload(add(base, 512))
                let a17 := calldataload(add(base, 544))
                let a18 := calldataload(add(base, 576))
                let a19 := calldataload(add(base, 608))
                selected := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17), a18), a19)
            }
            result := write(values, distance)
        }
    }
}
