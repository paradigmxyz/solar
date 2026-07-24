//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir

contract SignedConstantOps {
    function lt() public pure returns (bool) {
        return int256(-1) < int256(1);
    }

    function div() public pure returns (int256) {
        return int256(-7) / int256(2);
    }

    function shr() public pure returns (int256) {
        return int256(-8) >> 1;
    }
}
