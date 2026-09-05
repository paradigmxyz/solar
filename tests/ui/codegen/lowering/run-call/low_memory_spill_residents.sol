//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 0, 0 => 214
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 1, 0 => 428
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 2, 0 => 642
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 2, 1 => 642
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], 2, 2 => 642

contract LowMemorySpillResidents {
    function run(uint256[20] calldata values, uint256 depth, uint256 mode) external pure returns (uint256 result) {
        assembly {
            function walk(base, remaining, choice) -> selected {
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
                // These stores end at the scratch boundary and leave the live activation untouched.
                mstore(0, a00)
                mstore(32, a01)
                mstore(64, 0)
                mstore(96, a02)
                selected := add(mload(0), mload(32))
                switch choice
                case 1 {
                    // These writes cross or start at 0x80 and must remain spill barriers.
                    mstore(97, a03)
                    mstore8(127, a04)
                    mstore8(128, a05)
                }
                case 2 {
                    // Dynamic activations must preserve the private frame pointer across this write.
                    mstore(160, 0)
                }
                // A reset free-memory pointer must not let a child overlap an active frame.
                if remaining { selected := add(selected, walk(base, sub(remaining, 1), choice)) }
                selected := add(selected, add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17), a18), a19))
                mstore(64, 0)
                selected := add(selected, iszero(mload(64)))
                mstore(96, 0)
            }
            result := walk(values, and(depth, 3), mode)
        }
    }
}
