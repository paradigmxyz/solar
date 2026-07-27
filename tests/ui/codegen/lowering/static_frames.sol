//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:

// Static frame overlays after MIR inlining: the surviving non-recursive chain
// uses compile-time-fixed frame addresses, while recursive and mutually
// recursive calls share the dynamic frame allocator and epilogue.
contract SF {
    // CHECK: push 0x313ae541
    // CHECK: eq
    // CHECK-NEXT: push [[TOP:bb[0-9]+]]
    // CHECK: push 0x86b714e2
    // CHECK: eq
    // CHECK-NEXT: push [[GETTER:bb[0-9]+]]
    // CHECK: [[GETTER]]:
    // CHECK: sload
    // CHECK: jump [[RETURN:bb[0-9]+]]
    // CHECK: [[RETURN]]:
    // CHECK: return
    uint256 public s;

    // The optimized chainA/chainB/chainC path uses fixed addresses and does
    // not touch the dynamic frame pointer.
    // CHECK: [[TOP]]:
    // CHECK: push 480
    // CHECK-NEXT: mstore
    // CHECK: push 512
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push [[CHAIN_RET:bb[0-9]+]]
    // CHECK-NOT: push 160
    // CHECK: push 736
    // CHECK-NEXT: mstore
    // CHECK-NOT: push 160
    // CHECK: push 544
    // CHECK-NEXT: mstore
    // CHECK-NEXT: jump
    // CHECK-NEXT: [[CHAIN_RET]]:
    // CHECK-NEXT: push 544
    // CHECK-NEXT: mload
    // CHECK-NOT: push 160

    // Recursive calls allocate from the free-memory pointer and install a
    // dynamic frame pointer.
    // CHECK: push 7
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: mod
    // CHECK: push 64
    // CHECK-NEXT: mload
    // CHECK: push 160
    // CHECK-NEXT: mload
    // CHECK: push 32
    // CHECK-NEXT: add
    // CHECK-NEXT: mstore
    // CHECK: push 160
    // CHECK-NEXT: mstore
    // CHECK: push 64
    // CHECK-NEXT: mstore

    // Dynamic returns restore both pointers through a shared epilogue.
    // CHECK: bb67:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK: push 32
    // CHECK-NEXT: add
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mstore
    // CHECK-NEXT: jump
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
