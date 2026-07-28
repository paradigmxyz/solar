//@ compile-flags: -Zcodegen -O gas -Zdump=evm-ir-runtime
//@ filecheck:

contract OneFunction {
    // CHECK-LABEL: small_dispatch.sol:OneFunction (runtime) ===
    // CHECK: @module runtime
    // CHECK-NOT: indexed_jump
    function f0() external pure returns (uint256) {
        return 0;
    }
}

contract TwoFunctions {
    // CHECK-LABEL: small_dispatch.sol:TwoFunctions (runtime) ===
    // CHECK: @module runtime
    // CHECK-NOT: indexed_jump
    function f0() external pure returns (uint256) {
        return 0;
    }

    function f1() external pure returns (uint256) {
        return 1;
    }
}

contract ThreeFunctions {
    // CHECK-LABEL: small_dispatch.sol:ThreeFunctions (runtime) ===
    // CHECK: @module runtime
    // CHECK-NOT: indexed_jump
    function f0() external pure returns (uint256) {
        return 0;
    }

    function f1() external pure returns (uint256) {
        return 1;
    }

    function f2() external pure returns (uint256) {
        return 2;
    }
}

contract FourFunctions {
    // CHECK-LABEL: small_dispatch.sol:FourFunctions (runtime) ===
    // CHECK: @module runtime
    // CHECK-NOT: indexed_jump
    function f0() external pure returns (uint256) {
        return 0;
    }

    function f1() external pure returns (uint256) {
        return 1;
    }

    function f2() external pure returns (uint256) {
        return 2;
    }

    function f3() external pure returns (uint256) {
        return 3;
    }
}
