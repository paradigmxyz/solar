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
}
