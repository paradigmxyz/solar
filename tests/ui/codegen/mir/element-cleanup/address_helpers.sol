//@ compile-flags: -Zmir-pipeline=element-cleanup -Zpass-diff
//@ filecheck:
import {WordArrays} from "solar:core/v1/WordArrays.sol";

// The address helpers clean every element word they load. Without inline
// assembly no element can be dirty, so the masks go.
// CHECK-LABEL: :Clean (after element-cleanup)
// CHECK-LABEL: fn @core_array_has_duplicate_address
// CHECK: - {{v[0-9]+}} = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
// CHECK: - {{v[0-9]+}} = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
contract Clean {
    function hasDuplicate(address[] memory a) external pure returns (bool) {
        return WordArrays.hasDuplicate(a);
    }
}

// Assembly may leave an element dirty, so the masks stay.
// CHECK-LABEL: :Dirty (after element-cleanup)
// CHECK-LABEL: fn @core_array_has_duplicate_address
// CHECK-NOT: - {{v[0-9]+}} = and
contract Dirty {
    function hasDuplicate(uint256[] memory words) external pure returns (bool) {
        address[] memory a;
        assembly {
            a := words
        }
        return WordArrays.hasDuplicate(a);
    }
}
