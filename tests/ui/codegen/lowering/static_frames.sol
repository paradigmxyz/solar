//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

// Fixed multi-result storage coexists with stack-based scalar recursion and
// mutually recursive calls; the getter bypasses the allocating entry initialization.
contract SF {
    uint256 public s;

    // Scalar recursion keeps its arguments and return addresses on the physical stack.
    // CHECK-LABEL: @module SF_runtime
    // CHECK-NOT: push 64
    // CHECK: push 0x313ae541
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[TOP_SELECT:bb[0-9]+]], [[GETTER_SELECT:bb[0-9]+]]
    // CHECK: [[GETTER_SELECT]]:
    // CHECK-NEXT: push 0x86b714e2
    // CHECK-NEXT: sub
    // CHECK: sload
    // CHECK-NEXT: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 128
    // CHECK-NEXT: return
    // CHECK: push 7
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: mod
    // CHECK: push [[REC_CONT:bb[0-9]+]]
    // CHECK: jump [[REC_BODY:bb[0-9]+]]
    // CHECK: [[REC_BODY]]:
    // CHECK: [[REC_CONT]]:
    // CHECK-NEXT: push 5
    // The second chainC result uses a fixed buffer advertised through scratch word 32.
    // CHECK: mload
    // CHECK-NEXT: push 32
    // CHECK-NEXT: add
    // CHECK-NEXT: mload
    // CHECK: push 256
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 224
    // CHECK-NEXT: push 32
    // CHECK-NEXT: mstore
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: jump{{$}}
    // A recursive edge reuses the same body with a distinct suspended continuation.
    // CHECK: push {{bb[0-9]+}}
    // CHECK: jump [[REC_BODY]]
    // Only top's selected entry initializes the unchanged heap floor.
    // CHECK: [[TOP_SELECT]]:
    // CHECK: push 288
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK-NOT: push 64
    function top(uint256 x) external returns (uint256) {
        uint256 keep = x * 3; // live across all the calls below
        uint256 a = chainA(x);
        uint256 r = rec(x % 7, x);
        uint256 m = m1(x % 5, x);
        s += keep;
        return keep + a + r + m;
    }

    function chainA(uint256 x) internal returns (uint256) {
        uint256 la = x + 1; // live across chainB
        uint256 b = chainB(la, x);
        return la * 2 + b;
    }

    function chainB(uint256 la, uint256 x) internal returns (uint256) {
        uint256 lb = la ^ x;
        (uint256 c1, uint256 c2) = chainC(lb);
        s += c1;
        return lb + c1 * 2 + c2;
    }

    function chainC(uint256 lb) internal returns (uint256, uint256) {
        s += 1;
        return (lb / 3 + 1, lb % 5 + 2);
    }

    function rec(uint256 n, uint256 x) internal returns (uint256) {
        uint256 here = leafS(x + n);
        if (n == 0) {
            return here;
        }
        uint256 below = rec(n - 1, x + 1);
        return here + below + leafS(below);
    }

    function leafS(uint256 v) internal returns (uint256) {
        uint256 t = v * 2 + 1;
        s ^= t;
        return t % 1000;
    }

    function m1(uint256 n, uint256 x) internal returns (uint256) {
        if (n == 0) {
            return leaf2(x) + 7;
        }
        return leaf2(x) + m2(n - 1, x + 3);
    }

    function m2(uint256 n, uint256 x) internal returns (uint256) {
        if (n == 0) {
            return x % 13;
        }
        return m1(n - 1, x + 5) + 1;
    }

    function leaf2(uint256 x) internal returns (uint256) {
        s += x % 3;
        return x % 97;
    }
}
