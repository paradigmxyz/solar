//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Constant evaluation folds signed operations on constant operands.
contract SignedConstantOps {
    // CHECK-LABEL: fn @lt{{[( ]}}
    // CHECK: ret true
    function lt() public pure returns (bool) {
        return int256(-1) < int256(1);
    }

    // CHECK-LABEL: fn @div{{[( ]}}
    // CHECK: ret 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
    function div() public pure returns (int256) {
        return int256(-7) / int256(2);
    }

    // CHECK-LABEL: fn @shr{{[( ]}}
    // CHECK: ret 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc
    function shr() public pure returns (int256) {
        return int256(-8) >> 1;
    }

    // Operations with one constant operand lower to signed operations.
    // CHECK-LABEL: fn @ltParam{{[( ]}}
    // CHECK: slt 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, arg0
    function ltParam(int256 x) public pure returns (bool) {
        return int256(-1) < x;
    }

    // CHECK-LABEL: fn @divParam{{[( ]}}
    // CHECK: checked_div {{u?int[0-9]+}}, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff9, arg0
    function divParam(int256 x) public pure returns (int256) {
        return int256(-7) / x;
    }

    // CHECK-LABEL: fn @shrParam{{[( ]}}
    // CHECK: sar 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff8, arg0
    function shrParam(uint256 x) public pure returns (int256) {
        return int256(-8) >> x;
    }
}
