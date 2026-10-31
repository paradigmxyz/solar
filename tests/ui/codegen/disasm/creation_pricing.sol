//@ codegen-matrix: standard deploy runtime
//@[deploy] compile-flags: -Ogas -Zdump=disasm-deploy
//@[deploy] filecheck: --check-prefix=DEPLOY
//@[runtime] compile-flags: -Ogas -Zdump=disasm-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME
//@ run-call: Constants::v; constructor=[5] => 15
//@ run-call: Constants::a; constructor=[5] => 0x8ec1efb30a6414133f00e6192183fdfd6ba2e437
//@ run-call: Constants::c; constructor=[5] => 0x8ec1efb30a6414133f00e6192183fdfd6ba2e437
//@ run-call: Shared::v; constructor=[5] => 33
//@ run-call: Shared::f 2; constructor=[5] => 15
//@ run-call: Looped::arr 5; constructor=[1] => 18
//@ run-call: LoopAfterCall::g 7; constructor=[1] => 0x8c05d9c752a667a558b4610b5c58f90e92efda9fd08a96f913627e8565e98d3d
//@ run-call: LoopCalled::g 7; constructor=[1] => 0x175fbdd30d08f094be3685d051a42c7970231820fbefe8bf07de210357a206a5

// Creation code runs once and pays calldata for every byte, so gas builds weigh the bytes of the
// constructor against its execution gas at 16 to 1. The checked `p * 3` divides to test for
// overflow instead of comparing against a limit, and the address that three stores share
// stays on the stack after one push.
// DEPLOY-LABEL: // === {{.*}}:Constants (deployment) ===
// DEPLOY-NOT: PUSH32
// DEPLOY: DIV
// DEPLOY-NEXT: SUB
// DEPLOY: PUSH20 0x8ec1efb30a6414133f00e6192183fdfd6ba2e437
// DEPLOY-NOT: PUSH20
// DEPLOY: RETURN
contract Constants {
    uint256 public v;
    address public a;
    address public b;
    address public c;

    constructor(uint256 p) {
        v = p * 3;
        a = address(0x8EC1EfB30a6414133F00E6192183fdfD6bA2E437);
        b = address(0x8EC1EfB30a6414133F00E6192183fdfD6bA2E437);
        c = address(0x8EC1EfB30a6414133F00E6192183fdfD6bA2E437);
    }
}

// A helper that the runtime also calls is not creation-only code, so the constructor's copy of it
// and the runtime's both keep the gas-first overflow check against a limit. The
// constructor calls it twice, so inlining does not give the constructor a private copy.
// DEPLOY-LABEL: // === {{.*}}:Shared (deployment) ===
// DEPLOY: DIV
// DEPLOY-NEXT: LT
// RUNTIME-LABEL: // === {{.*}}:Shared (runtime) ===
// RUNTIME: DIV
// RUNTIME-NEXT: LT
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
// limit.
// DEPLOY-LABEL: // === {{.*}}:Looped (deployment) ===
// DEPLOY: DIV
// DEPLOY-NEXT: LT
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
// DEPLOY-COUNT-4: PUSH32 0x306b6a7bc62143ed7aa6bc48f76bfab65859a6c8f14f9b05ec52780106d96515
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
            r = (x ^ 0x306b6a7bc62143ed7aa6bc48f76bfab65859a6c8f14f9b05ec52780106d96515) + total;
            total = r ^ 0x306b6a7bc62143ed7aa6bc48f76bfab65859a6c8f14f9b05ec52780106d96515;
            r += (x * 3) ^ 0x306b6a7bc62143ed7aa6bc48f76bfab65859a6c8f14f9b05ec52780106d96515;
            total += (r * 5) ^ 0x306b6a7bc62143ed7aa6bc48f76bfab65859a6c8f14f9b05ec52780106d96515;
        }
    }

    function g(uint256 x) external returns (uint256) {
        return h(x);
    }
}

// The code after a call in a constructor loop runs once per iteration too, so the constants it
// uses stay on the stack from one push before the loop instead of being shared through a stub.
// DEPLOY-LABEL: // === {{.*}}:LoopAfterCall (deployment) ===
// DEPLOY: PUSH32 0xbd0025031d4a40ca7316715da312db483116de9ad150e22b438734abe4a0ee22
// DEPLOY-NEXT: PUSH32 0x15fcbca373ac4001f6fd95867a8a1997e09c261b43b56b4333d418a2c28ce221
// DEPLOY-NEXT: PUSH32 0xf8d5635289df23d34e3ac8e12342e8d032dc433a35f2c968a4af0a4873086206
// DEPLOY-NEXT: PUSH32 0x306b6a7bc62143ed7aa6bc48f76bfab65859a6c8f14f9b05ec52780106d96515
contract LoopAfterCall {
    uint256[50] public arr;
    uint256 public total;

    constructor(uint256 s) {
        for (uint256 i; i < 50; ++i) {
            uint256 r = h(s + i);
            unchecked {
                r = (r ^ 0x306b6a7bc62143ed7aa6bc48f76bfab65859a6c8f14f9b05ec52780106d96515)
                    + (r * 3 ^ 0xf8d5635289df23d34e3ac8e12342e8d032dc433a35f2c968a4af0a4873086206)
                    + (r * 5 ^ 0x15fcbca373ac4001f6fd95867a8a1997e09c261b43b56b4333d418a2c28ce221)
                    + (r * 7 ^ 0xbd0025031d4a40ca7316715da312db483116de9ad150e22b438734abe4a0ee22);
                arr[i] = r ^ 0x306b6a7bc62143ed7aa6bc48f76bfab65859a6c8f14f9b05ec52780106d96515
                    ^ 0xf8d5635289df23d34e3ac8e12342e8d032dc433a35f2c968a4af0a4873086206
                    ^ 0x15fcbca373ac4001f6fd95867a8a1997e09c261b43b56b4333d418a2c28ce221;
                total += (r + 1 ^ 0xbd0025031d4a40ca7316715da312db483116de9ad150e22b438734abe4a0ee22)
                    + (r + 2 ^ 0x306b6a7bc62143ed7aa6bc48f76bfab65859a6c8f14f9b05ec52780106d96515)
                    + (r + 3 ^ 0xf8d5635289df23d34e3ac8e12342e8d032dc433a35f2c968a4af0a4873086206)
                    + (r + 4 ^ 0x15fcbca373ac4001f6fd95867a8a1997e09c261b43b56b4333d418a2c28ce221);
            }
        }
    }

    function h(uint256 x) internal returns (uint256 r) {
        unchecked {
            total += x;
            r = total * 7 + x;
        }
    }

    function g(uint256 x) external returns (uint256) {
        return h(x);
    }
}
