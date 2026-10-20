//@ codegen-matrix: standard deploy runtime
//@[deploy] compile-flags: -Ogas -Zdump=disasm-deploy
//@[deploy] filecheck: --check-prefix=DEPLOY
//@[runtime] compile-flags: -Ogas -Zdump=disasm-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME
//@ run-call: Constants::v; constructor=[5] => 15
//@ run-call: Constants::a; constructor=[5] => 0x1111111111111111111111111111111111111111
//@ run-call: Constants::c; constructor=[5] => 0x1111111111111111111111111111111111111111
//@ run-call: Shared::v; constructor=[5] => 33
//@ run-call: Shared::f 2; constructor=[5] => 15
//@ run-call: Looped::arr 5; constructor=[1] => 18
//@ run-call: LoopCalled::g 7; constructor=[1] => 0xe5911188c23b5df9927b4d3fc62cc87e06c9374664016f91a70f29bdb41a4252

// Creation code runs once and pays calldata for every byte, so gas builds weigh the bytes of the
// constructor against its execution gas at 16 to 1. The checked `p * 3` divides to test for
// overflow instead of comparing against a 32-byte limit, and the address that three stores share
// stays on the stack after one push.
// DEPLOY-LABEL: // === {{.*}}:Constants (deployment) ===
// DEPLOY-NOT: PUSH32
// DEPLOY: DIV
// DEPLOY: PUSH20 0x1111111111111111111111111111111111111111
// DEPLOY-NOT: PUSH20
// DEPLOY: RETURN
contract Constants {
    uint256 public v;
    address public a;
    address public b;
    address public c;

    constructor(uint256 p) {
        v = p * 3;
        a = address(0x1111111111111111111111111111111111111111);
        b = address(0x1111111111111111111111111111111111111111);
        c = address(0x1111111111111111111111111111111111111111);
    }
}

// A helper that the runtime also calls is not creation-only code, so the constructor's copy of it
// and the runtime's both keep the gas-first overflow check against a 32-byte limit. The
// constructor calls it twice, so inlining does not give the constructor a private copy.
// DEPLOY-LABEL: // === {{.*}}:Shared (deployment) ===
// DEPLOY: PUSH32 0x5555555555555555555555555555555555555555555555555555555555555555
// RUNTIME-LABEL: // === {{.*}}:Shared (runtime) ===
// RUNTIME: PUSH32 0x5555555555555555555555555555555555555555555555555555555555555555
contract Shared {
    uint256 public v;

    constructor(uint256 p) {
        v = triple(p) + triple(p + 1);
    }

    function triple(uint256 p) internal pure returns (uint256) {
        return p * 3;
    }

    function f(uint256 p) external pure returns (uint256) {
        return triple(p) + triple(p + 1);
    }
}

// A loop body runs once per iteration, so a loop in the constructor keeps the check against a
// 32-byte limit.
// DEPLOY-LABEL: // === {{.*}}:Looped (deployment) ===
// DEPLOY: PUSH32 0x5555555555555555555555555555555555555555555555555555555555555555
contract Looped {
    uint256[100] public arr;

    constructor(uint256 s) {
        for (uint256 i; i < 100; ++i) {
            arr[i] = (s + i) * 3;
        }
    }
}

// A helper that a constructor loop calls runs once per iteration, so it keeps its four pushes of
// the constant instead of sharing them through a stub that costs gas on every call.
// DEPLOY-LABEL: // === {{.*}}:LoopCalled (deployment) ===
// DEPLOY-COUNT-4: PUSH32 0x1234567812345678123456781234567812345678123456781234567812345678
contract LoopCalled {
    uint256[50] public arr;
    uint256 public total;

    constructor(uint256 s) {
        for (uint256 i; i < 50; ++i) {
            unchecked {
                arr[i] = h(s + i) + h(s + 2 * i) + h(s + 3 * i);
            }
        }
    }

    function h(uint256 x) internal returns (uint256 r) {
        unchecked {
            r = (x ^ 0x1234567812345678123456781234567812345678123456781234567812345678) + total;
            total = r ^ 0x1234567812345678123456781234567812345678123456781234567812345678;
            r += (x * 3) ^ 0x1234567812345678123456781234567812345678123456781234567812345678;
            total += (r * 5) ^ 0x1234567812345678123456781234567812345678123456781234567812345678;
        }
    }

    function g(uint256 x) external returns (uint256) {
        return h(x);
    }
}
