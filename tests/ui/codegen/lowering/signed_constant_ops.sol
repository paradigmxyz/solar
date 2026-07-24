//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract SignedConstantOps {
    // CHECK-LABEL: fn @lt
    // CHECK: [[NEG_ONE:v[0-9]+]] = sub 0, 1
    // CHECK: slt [[NEG_ONE]], 1
    function lt() public pure returns (bool) {
        return int256(-1) < int256(1);
    }

    // CHECK-LABEL: fn @div
    // CHECK: [[NEG_SEVEN:v[0-9]+]] = sub 0, 7
    // CHECK: sdiv [[NEG_SEVEN]], 2
    function div() public pure returns (int256) {
        return int256(-7) / int256(2);
    }

    // CHECK-LABEL: fn @shr
    // CHECK: [[NEG_EIGHT:v[0-9]+]] = sub 0, 8
    // CHECK: sar 1, [[NEG_EIGHT]]
    function shr() public pure returns (int256) {
        return int256(-8) >> 1;
    }
}
