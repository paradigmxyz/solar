//@ codegen-matrix: standard sizemir
//@[sizemir] compile-flags: -Osize -Zdump=mir
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[sizemir] normalize-stdout-test: "(?s).+" -> ""
//@[sizemir] filecheck: --check-prefix=SIZE
//@ run-call: sortInts [2, -3, 1] => [-3, 1, 2]
//@ run-call: sortInts [0, -1, 7, -1] => [-1, -1, 0, 7]

import {WordArrays} from "solar:core/v1/WordArrays.sol";

// The only caller of the shared sort passes a zero pair distance and a sign
// flip. Substituting both does not pay: the flip is a wide literal at every
// comparison. Size builds then substitute the zero alone, which folds the
// paired moves away, and the flip stays an argument.
contract C {
    // SIZE: icall @core_array_group_sort, {{v[0-9]+}}, {{v[0-9]+}}, 0x8000000000000000000000000000000000000000000000000000000000000000{{$}}
    // SIZE: fn @core_array_group_sort(arg0: i256, arg1: i256, arg2: i256) {
    function sortInts(int256[] memory a) public pure returns (int256[] memory) {
        WordArrays.sort(a);
        return a;
    }
}
