//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract SignedConstantOps {
    // CHECK-LABEL: fn @lt{{[( ]}}
    // CHECK: slt 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1
    function lt() public pure returns (bool) {
        return int256(-1) < int256(1);
    }

    // CHECK-LABEL: fn @div{{[( ]}}
    // CHECK: sdiv 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff9, 2
    function div() public pure returns (int256) {
        return int256(-7) / int256(2);
    }

    // CHECK-LABEL: fn @shr{{[( ]}}
    // CHECK: sar 1, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff8
    function shr() public pure returns (int256) {
        return int256(-8) >> 1;
    }
}
