//@ revisions: gas size runtime
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[gas] filecheck:
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefixes=CHECK,SIZE
//@[runtime] compile-flags: -O size
//@[runtime] run-call: PartialTerminalDispatch::f0 => 0
//@[runtime] run-call: PartialTerminalDispatch::f1
//@[runtime] run-call: PartialTerminalDispatch::f39

contract OneFunction {
    // CHECK-LABEL: small_dispatch.sol:OneFunction (runtime) ===
    // CHECK: @module OneFunction_runtime
    // CHECK-NOT: indexed_jump
    function f0() external pure returns (uint256) {
        return 0;
    }
}

contract TwoFunctions {
    // CHECK-LABEL: small_dispatch.sol:TwoFunctions (runtime) ===
    // CHECK: @module TwoFunctions_runtime
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
    // CHECK: @module ThreeFunctions_runtime
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
    // CHECK: @module FourFunctions_runtime
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

contract PartialTerminalDispatch {
    // SIZE-LABEL: small_dispatch.sol:PartialTerminalDispatch (runtime) ===
    // SIZE-COUNT-2: gt
    // SIZE-NOT: gt
    // SIZE: eq
    // SIZE-NEXT: push [[STOP:bb[0-9]+]]
    // SIZE-NEXT: jumpi
    // SIZE-NOT: gt
    // SIZE: [[STOP]]:
    // SIZE-NEXT: stop
    // SIZE: gt
    // SIZE-NOT: gt
    // SIZE: eq
    // SIZE-NEXT: push [[STOP]]
    // SIZE-NEXT: jumpi
    // SIZE-NOT: gt
    function f0() external view returns (uint256) {
        (bool success,) = address(this).staticcall(hex"ffffffff");
        return success ? 1 : 0;
    }

    function f1() external {}
    function f2() external {}
    function f3() external {}
    function f4() external {}
    function f5() external {}
    function f6() external {}
    function f7() external {}
    function f8() external {}
    function f9() external {}
    function f10() external {}
    function f11() external {}
    function f12() external {}
    function f13() external {}
    function f14() external {}
    function f15() external {}
    function f16() external {}
    function f17() external {}
    function f18() external {}
    function f19() external {}
    function f20() external {}
    function f21() external {}
    function f22() external {}
    function f23() external {}
    function f24() external {}
    function f25() external {}
    function f26() external {}
    function f27() external {}
    function f28() external {}
    function f29() external {}
    function f30() external {}
    function f31() external {}
    function f32() external {}
    function f33() external {}
    function f34() external {}
    function f35() external {}
    function f36() external {}
    function f37() external {}
    function f38() external {}
    function f39() external {}
}
