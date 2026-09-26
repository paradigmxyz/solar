//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: roundTrip 0, 0x11 => 0x1100000000000000000000000000000000000000000000000000000000000000
//@ run-call: roundTrip 7, 0x22 => 0x2200000000000000000000000000000000000000000000000000000000000000
//@ run-call: roundTrip 18446744073709551615, 0x33 => 0x3300000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: roundTrip 18446744073709551616, 0x44 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: loadAt 18446744073709551616 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: sameAsArray 3, 0x55 => true
//@ run-call: rootIsOwn 0x66 => 0x6600000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: nested 2, 0x77 => 0x7700000000000000000000000000000000000000000000000000000000000000
//@ run-call: storeThenWords 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728, 0, 40 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 0x2122232425262728000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: storeThenWords 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728, 5, 30 => 0x060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122230000, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: storeCalldataThenWords 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728, 0, 40 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 0x2122232425262728000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: storeCalldataThenWords 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728, 5, 30 => 0x060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122230000, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: roundTripBytes 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728, 2 => 0xeeee0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728eeeeee
//@ run-call-fail: storeThenWords 0x0102, 1, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: storeThenWords 0x01, 0, 590295810358705651712 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: storeCalldataThenWords 0x0102, 2, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: loadPastEnd 0x010203 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

import {Slots} from "solar:core/v1/Slots.sol";

contract Safe {
    struct Wrapper {
        Slots.Root root;
    }

    uint256 private before;
    Slots.Root private plain;
    Wrapper private wrapped;

    function roundTrip(uint256 index, bytes1 value) public returns (bytes32) {
        Slots.store(plain, index, bytes32(value));
        return Slots.load(plain, index);
    }

    function loadAt(uint256 index) public view returns (bytes32) {
        return Slots.load(plain, index);
    }

    // Word `index` is where a dynamic array at the root's slot keeps element `index`.
    function sameAsArray(uint256 index, bytes1 value) public returns (bool) {
        Slots.store(plain, index, bytes32(value));
        bytes32 viaSlot;
        assembly ("memory-safe") {
            mstore(0x00, plain.slot)
            viaSlot := sload(add(keccak256(0x00, 0x20), index))
        }
        return viaSlot == bytes32(value);
    }

    // The root word is the owner's; derived words leave it alone.
    function rootIsOwn(bytes1 value) public returns (bytes32, bytes32) {
        plain.word = bytes32(value);
        Slots.store(plain, 0, bytes32(0));
        return (plain.word, Slots.load(plain, 0));
    }

    // A root inside a struct roots the region of its own slot.
    function nested(uint256 index, bytes1 value) public returns (bytes32) {
        Slots.store(wrapped.root, index, bytes32(value));
        bytes32 viaSlot;
        assembly ("memory-safe") {
            mstore(0x00, wrapped.slot)
            viaSlot := sload(add(keccak256(0x00, 0x20), index))
        }
        return viaSlot;
    }

    function storeThenWords(bytes memory b, uint256 offset, uint256 count)
        public
        returns (bytes32, bytes32, bytes32)
    {
        Slots.storeBytes(plain, b, offset, count);
        return (_derived(0), _derived(1), _derived(2));
    }

    function storeCalldataThenWords(bytes calldata b, uint256 offset, uint256 count)
        public
        returns (bytes32, bytes32, bytes32)
    {
        Slots.storeCalldataBytes(plain, b, offset, count);
        return (_derived(0), _derived(1), _derived(2));
    }

    // Bytes of the buffer outside the range keep what they held.
    function roundTripBytes(bytes memory b, uint256 offset) public returns (bytes memory) {
        Slots.storeBytes(plain, b, 0, b.length);
        bytes memory out = new bytes(b.length + offset + 3);
        for (uint256 i; i < out.length; ++i) {
            out[i] = 0xee;
        }
        Slots.loadBytes(plain, out, offset, b.length);
        return out;
    }

    // A range that runs past the buffer fails before any write.
    function loadPastEnd(bytes memory b) public view returns (bytes memory) {
        Slots.loadBytes(plain, b, 1, b.length);
        return b;
    }

    function _derived(uint256 k) private view returns (bytes32 v) {
        assembly ("memory-safe") {
            mstore(0x00, plain.slot)
            v := sload(add(keccak256(0x00, 0x20), k))
        }
    }
}
