//@ revisions: none gas size runtime
//@[none] compile-flags: -O none -Zdump=evm-ir-runtime
//@[none] filecheck: --check-prefix=NONE
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefix=GAS
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefix=SIZE
//@[runtime] compile-flags: -O gas
//@[runtime] run-call: hashBranch false, 1, 2 => 0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6
//@[runtime] run-call: hashBranch true, 1, 2 => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0

// Each branch stores its free-memory-pointer load before updating the pointer, but the loaded value
// does not escape the branch. Gas and unoptimized lowering should therefore reuse one block-local
// spill slot. Size lowering deliberately keeps both slots stable.
//
// NONE-LABEL: @module runtime
// NONE-NEXT: bb0:
// NONE-NEXT: push 192
// NONE-NEXT: push 64
// NONE-NEXT: mstore
// NONE: bb11:
// NONE-NEXT: push 64
// NONE-NEXT: mload
// NONE-NEXT: dup1
// NONE-NEXT: push [[NONE_FMP_SLOT:[0-9]+]]
// NONE-NEXT: mstore
// NONE: bb12:
// NONE-NEXT: push 64
// NONE-NEXT: mload
// NONE-NEXT: dup1
// NONE-NEXT: push [[NONE_FMP_SLOT]]
// NONE-NEXT: mstore
//
// GAS-LABEL: @module runtime
// GAS-NEXT: bb0:
// GAS-NEXT: push 192
// GAS-NEXT: push 64
// GAS-NEXT: mstore
// GAS: pop
// GAS-NEXT: push 64
// GAS-NEXT: mload
// GAS-NEXT: dup1
// GAS-NEXT: push [[GAS_FMP_SLOT:[0-9]+]]
// GAS-NEXT: mstore
// GAS: bb10:
// GAS-NEXT: push 64
// GAS-NEXT: mload
// GAS-NEXT: dup1
// GAS-NEXT: push [[GAS_FMP_SLOT]]
// GAS-NEXT: mstore
//
// SIZE-LABEL: @module runtime
// SIZE-NEXT: bb0:
// SIZE-NEXT: push 224
// SIZE-NEXT: push 64
// SIZE-NEXT: mstore
// SIZE: pop
// SIZE-NEXT: push 64
// SIZE-NEXT: mload
// SIZE-NEXT: dup1
// SIZE-NEXT: push 192
// SIZE-NEXT: mstore
// SIZE: bb10:
// SIZE-NEXT: push 64
// SIZE-NEXT: mload
// SIZE-NEXT: dup1
// SIZE-NEXT: push 160
// SIZE-NEXT: mstore
contract FmpBlockLocalSpills {
    function hashBranch(
        bool pair,
        uint256 a,
        uint256 b
    ) external pure returns (bytes32 result) {
        assembly {
            switch pair
            case 0 {
                let p := mload(0x40)
                mstore(0x40, add(p, 0x20))
                mstore(p, a)
                result := keccak256(p, 0x20)
            }
            default {
                let p := mload(0x40)
                mstore(0x40, add(p, 0x40))
                mstore(p, a)
                mstore(add(p, 0x20), b)
                result := keccak256(p, 0x40)
            }
        }
    }
}
