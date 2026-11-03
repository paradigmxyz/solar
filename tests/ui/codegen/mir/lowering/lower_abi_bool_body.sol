//@ compile-flags: -Zmir-pipeline=lower-abi -Zdump=mir
//@ filecheck:

// A public function that is called internally keeps one body, whose bool parameter is the word
// the entry checked to be 0 or 1. The wrapper passes that word and an internal caller passes
// `zext b`; the body tests the word where it reads the bool.
contract BoolBody {
    // CHECK-LABEL: fn @negated()
    // CHECK: [[WORD:v[0-9]+]] = zext i1 {{v[0-9]+}} to i256
    // CHECK: icall @pick.body, [[WORD]]
    function negated(bool a, uint256 x, uint256 y) external pure returns (uint256) {
        return pick(!a, x, y);
    }

    // CHECK-LABEL: fn @pick.body(arg0: i256, arg1: i256, arg2: i256)
    // CHECK: [[TEST:v[0-9]+]] = ne arg0, 0
    // CHECK: jumpi [[TEST]]
    function pick(bool a, uint256 x, uint256 y) public pure returns (uint256) {
        return a ? x : y;
    }
}
