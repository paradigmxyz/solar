//@ revisions: size none
//@[size] compile-flags: -Osize -Zdump=mir
//@[size] filecheck:
//@[none] compile-flags: -O none
//@ run-call: sortInts [3, -1, 2, -7, 0, 3] => [-7, -1, 0, 2, 3, 3]
//@ run-call: sortInts [] => []
//@ run-call: sortInts [5] => [5]
//@ run-call: sortInts [2, 1] => [1, 2]
//@ run-call: sortInts [57896044618658097711785492504343953926634992332820282019728792003956564819967, -57896044618658097711785492504343953926634992332820282019728792003956564819968, 0, -1] => [-57896044618658097711785492504343953926634992332820282019728792003956564819968, -1, 0, 57896044618658097711785492504343953926634992332820282019728792003956564819967]
//@ run-call: sortInts [9, 8, 7, 6, 5, 4, 3, 2, 1, 0, -1, -2, -3, -4, -5, -6, -7] => [-7, -6, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
//@ run-call: groupSums [3, 1, 3, 2, 1], [10, 20, 30, 40, 50] => [1, 2, 3], [70, 40, 40]
//@ run-call: groupSums [4, 4, 4], [1, 2, 3] => [4], [6]
//@ run-call: groupSums [1], [9] => [1], [9]

import {WordArrays} from "solar:core/v1/WordArrays.sol";

// Builds that do not optimize for gas share one paired quicksort between
// plain sorts and `groupSum`. A plain sort passes a zero pair distance and
// flips signed keys by `2**255`; `groupSum` passes the distance from its keys
// to its values and no flip.
// CHECK-LABEL: fn @sortInts
// CHECK: icall @core_array_group_sort, {{v[0-9]+}}, {{v[0-9]+}}, 0, 0x8000000000000000000000000000000000000000000000000000000000000000
// CHECK-LABEL: fn @core_array_group_sum
// CHECK: icall @core_array_group_sort, {{v[0-9]+}}, {{v[0-9]+}}, {{v[0-9]+}}, 0
contract Test {
    function sortInts(int256[] memory a) public pure returns (int256[] memory) {
        WordArrays.sort(a);
        return a;
    }

    function groupSums(uint256[] memory keys, uint256[] memory values)
        public
        pure
        returns (uint256[] memory, uint256[] memory)
    {
        WordArrays.groupSum(keys, values);
        return (keys, values);
    }
}
