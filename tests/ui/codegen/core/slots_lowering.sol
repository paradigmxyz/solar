//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Slots.load` and `Slots.store` hash the root's slot in scratch memory, test
// the index against 2**64 and add it, where a dynamic array at that slot keeps
// its elements. The hash comes first, so a loop over one region can hoist it.
// The shipped bodies do the same in assembly and stay calls.
// INTRINSIC-LABEL: fn @get
// INTRINSIC: keccak256 0, 32
// INTRINSIC: shr 64,
// INTRINSIC: sload
// INTRINSIC-NOT: icall @load
// INTRINSIC-LABEL: fn @set
// INTRINSIC: keccak256 0, 32
// INTRINSIC: sstore
// PORTABLE-LABEL: fn @get
// PORTABLE: icall @load

// `Slots.storeBytes` hashes the root's slot once and stores each whole word
// without a check of its own; the range and the count are checked up front.
// INTRINSIC-LABEL: fn @put
// INTRINSIC: keccak256 0, 32
// INTRINSIC-NOT: keccak256
// INTRINSIC: sstore
// INTRINSIC-NOT: keccak256
// INTRINSIC: stop
import {Slots} from "solar:core/v1/Slots.sol";

contract Test {
    Slots.Root root;

    function get(uint256 index) public view returns (bytes32) {
        return Slots.load(root, index);
    }

    function set(uint256 index, bytes32 value) public {
        Slots.store(root, index, value);
    }

    function put(bytes memory b) public {
        Slots.storeBytes(root, b, 0, b.length);
    }
}
