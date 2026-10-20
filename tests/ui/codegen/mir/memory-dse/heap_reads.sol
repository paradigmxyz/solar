//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: load => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627
//@ run-call: hashes 2 => [0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563, 0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6]
//@ run-call: hashes 0 => []
//@ run-call: wrapped => 7
//@ run-call: below 1 => 7

// The heap lies above the scratch words and the free memory pointer, so a read through a heap
// pointer observes neither. When only such reads follow before the call halts, the scratch word
// a folded hash of a storage slot wrote and the free memory pointer an allocation bumped are dead.

// CHECK-LABEL: fn @load(
// CHECK-NOT: mstore 0, 0
// CHECK-NOT: mstore 64,
// CHECK: returndata

// CHECK-LABEL: fn @hashes(
// CHECK-NOT: mstore 64,
// CHECK: returndata

// An index past a length forged in assembly can wrap an element read around to the scratch
// word, and assembly can compute an address below the heap, so both keep it.
// CHECK-LABEL: fn @wrapped(
// CHECK: mstore 0, 7
// CHECK: mstore 64,

// CHECK-LABEL: fn @below(
// CHECK: mstore 0, 7
// CHECK: mstore 64,

contract Test {
    bytes data;

    constructor() {
        data = hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627";
    }

    function load() external view returns (bytes memory) {
        return data;
    }

    function hashes(uint256 n) external pure returns (bytes32[] memory out) {
        out = new bytes32[](n);
        for (uint256 i; i < n; ++i) {
            out[i] = keccak256(abi.encode(i));
        }
    }

    function wrapped() external pure returns (uint256 r) {
        assembly {
            mstore(0, 7)
        }
        uint256[] memory a = new uint256[](1);
        uint256 i;
        assembly {
            mstore(a, not(0))
            i := div(sub(0, add(a, 0x20)), 0x20)
        }
        r = a[i];
    }

    function below(uint256 scale) external pure returns (uint256 r) {
        assembly {
            mstore(0, 7)
        }
        bytes memory b = new bytes(1);
        assembly {
            r := mload(sub(b, mul(b, scale)))
        }
    }
}
