//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// `Slots.load` and `Slots.store` hash the root's slot in scratch memory, test
// the index against 2**64 and add it, where a dynamic array at that slot keeps
// its elements. The hash comes first, so a loop over one region can hoist it,
// and the hash of a constant root folds to a constant. The shipped bodies do
// the same in assembly, and inline.
// INTRINSIC-LABEL: fn @get
// INTRINSIC-NOT: keccak256
// INTRINSIC: shr arg0, 64
// INTRINSIC: sload
// INTRINSIC-NOT: icall @load
// INTRINSIC-LABEL: fn @set
// INTRINSIC-NOT: keccak256
// INTRINSIC: sstore
// PORTABLE-LABEL: fn @get
// PORTABLE: shr arg0, 64
// PORTABLE: sload

// `Slots.storeBytes` stores each whole word from a source cursor to a slot
// stepping from the hash of the root's slot, without a check of its own; the
// range and the count are checked up front. The hash starts the slot, and the
// rest of the range goes to the slot where the loop stops.
// INTRINSIC-LABEL: fn @put
// INTRINSIC: [[SLOT:v[0-9]+]] = phi [{{bb[0-9]+}}: 0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563]
// INTRINSIC: sstore [[SLOT]], {{v[0-9]+}}
// INTRINSIC: stop
// INTRINSIC: sstore [[SLOT]], {{v[0-9]+}}
// INTRINSIC-NOT: keccak256
import {Slots} from "solar:core/Slots.sol";

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
