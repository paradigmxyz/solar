//@ compile-flags: -O gas -Zdump=evm-ir-runtime -Zswitch-max-gas-code-growth=0
//@ normalize-stdout-test: "(?s).+" -> ""
//@ filecheck:

// The external selector switch weighs its code against the expected calls, so
// forty routes still leave the linear scan for a bucket table when the
// artifact-wide growth budget that bounds other switches is exhausted.
contract SelectorGrowthBudget {
    // CHECK-LABEL: @module SelectorGrowthBudget_runtime
    // CHECK: shr
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: push
    // CHECK-NEXT: and
    // CHECK-NEXT: indexed_jump
    function f00() external pure returns (uint256) {
        return 0;
    }

    function f01() external pure returns (uint256) {
        return 1;
    }

    function f02() external pure returns (uint256) {
        return 2;
    }

    function f03() external pure returns (uint256) {
        return 3;
    }

    function f04() external pure returns (uint256) {
        return 4;
    }

    function f05() external pure returns (uint256) {
        return 5;
    }

    function f06() external pure returns (uint256) {
        return 6;
    }

    function f07() external pure returns (uint256) {
        return 7;
    }

    function f08() external pure returns (uint256) {
        return 8;
    }

    function f09() external pure returns (uint256) {
        return 9;
    }

    function f10() external pure returns (uint256) {
        return 10;
    }

    function f11() external pure returns (uint256) {
        return 11;
    }

    function f12() external pure returns (uint256) {
        return 12;
    }

    function f13() external pure returns (uint256) {
        return 13;
    }

    function f14() external pure returns (uint256) {
        return 14;
    }

    function f15() external pure returns (uint256) {
        return 15;
    }

    function f16() external pure returns (uint256) {
        return 16;
    }

    function f17() external pure returns (uint256) {
        return 17;
    }

    function f18() external pure returns (uint256) {
        return 18;
    }

    function f19() external pure returns (uint256) {
        return 19;
    }

    function f20() external pure returns (uint256) {
        return 20;
    }

    function f21() external pure returns (uint256) {
        return 21;
    }

    function f22() external pure returns (uint256) {
        return 22;
    }

    function f23() external pure returns (uint256) {
        return 23;
    }

    function f24() external pure returns (uint256) {
        return 24;
    }

    function f25() external pure returns (uint256) {
        return 25;
    }

    function f26() external pure returns (uint256) {
        return 26;
    }

    function f27() external pure returns (uint256) {
        return 27;
    }

    function f28() external pure returns (uint256) {
        return 28;
    }

    function f29() external pure returns (uint256) {
        return 29;
    }

    function f30() external pure returns (uint256) {
        return 30;
    }

    function f31() external pure returns (uint256) {
        return 31;
    }

    function f32() external pure returns (uint256) {
        return 32;
    }

    function f33() external pure returns (uint256) {
        return 33;
    }

    function f34() external pure returns (uint256) {
        return 34;
    }

    function f35() external pure returns (uint256) {
        return 35;
    }

    function f36() external pure returns (uint256) {
        return 36;
    }

    function f37() external pure returns (uint256) {
        return 37;
    }

    function f38() external pure returns (uint256) {
        return 38;
    }

    function f39() external pure returns (uint256) {
        return 39;
    }
}
