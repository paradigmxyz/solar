//@ revisions: none gas size runtime
//@[none] compile-flags: -O none -Zdump=evm-ir-runtime
//@[none] filecheck: --check-prefix=NONE
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefix=GAS
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefix=SIZE
//@[runtime] compile-flags: -O gas
//@ run-call: hashBranch false, 1, 2 => 0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6
//@ run-call: hashBranch true, 1, 2 => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0

// The free-memory pointer read in each switch arm stays on the stack, so no
// level reserves a spill slot for it and the frame floor stays at 160.
// NONE-LABEL: @module FmpBlockLocalSpills_runtime
// NONE: push 160
// NONE-NEXT: push 64
// NONE-NEXT: mstore
// NONE: push 64
// NONE-NEXT: mload
// NONE-NEXT: push 32
// NONE: keccak256
// NONE: push 64
// NONE-NEXT: mload
// NONE-NEXT: push 64
//
// GAS-LABEL: @module FmpBlockLocalSpills_runtime
// GAS: push 160
// GAS-NEXT: push 64
// GAS-NEXT: mstore
// GAS: mload
// GAS-NEXT: push 32
// GAS: mload
// GAS-NEXT: push 64
//
// SIZE-LABEL: @module FmpBlockLocalSpills_runtime
// SIZE: push 160
// SIZE-NEXT: push 64
// SIZE-NEXT: mstore
// SIZE: mload
// SIZE-NOT: mstore
// SIZE: push 64{{[[:space:]]+}}mstore
// SIZE: mload
// SIZE-NOT: mstore
// SIZE: push 64{{[[:space:]]+}}mstore
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
