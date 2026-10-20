//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: load => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627
//@ run-call: loadScaled 2 => 0x00020406080a0c0e10121416181a1c1e20222426282a2c2e30323436383a3c3e40424446484a4c4e
//@ run-call: loadThenAllocate => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 0x010000

// A helper that copies a long storage value into a fresh buffer writes the scratch word for the
// value's data slot, whose hash folds to a constant. Its callers read only the heap after the
// call, the buffer it returns included, so the scratch store is dead. One caller allocates
// after the call, so the free memory pointer bump stays.

// CHECK-LABEL: fn @_copy(
// CHECK: mstore 64,
// CHECK-NOT: mstore 0, 0{{( |$)}}
// CHECK-LABEL: fn @clear_storage_words(

contract Test {
    bytes data;

    constructor() {
        data = hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627";
    }

    function load() external view returns (bytes memory) {
        return _copy(1);
    }

    function loadThenAllocate() external view returns (bytes memory a, bytes memory b) {
        a = _copy(1);
        assembly ("memory-safe") {
            b := mload(0x40)
            mstore(b, 3)
            mstore(add(b, 0x20), shl(248, 1))
            mstore(0x40, add(b, 0x40))
        }
    }

    function loadScaled(uint256 k) external view returns (bytes memory) {
        return _copy(k);
    }

    function _copy(uint256 k) internal view returns (bytes memory out) {
        bytes storage src = data;
        uint256 n = src.length;
        out = new bytes(n);
        for (uint256 i; i < n; ++i) {
            out[i] = bytes1(uint8(src[i]) * uint8(k));
        }
    }
}
