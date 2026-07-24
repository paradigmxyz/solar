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

    // The optimized chainA/chainB/chainC path has one surviving static call.
    // CHECK: [[TOP]]:
    // CHECK: push 672
    // CHECK-NEXT: mstore
    // CHECK: push 704
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push [[CHAIN_RET:bb[0-9]+]]
    // CHECK-NOT: push 160
    // CHECK: push 928
    // CHECK-NEXT: mstore
    // CHECK-NOT: push 160
    // CHECK: push 736
    // CHECK-NEXT: mstore
    // CHECK-NEXT: jump
    // CHECK-NEXT: [[CHAIN_RET]]:
    // CHECK-NEXT: push 736
    // CHECK-NEXT: mload
    // CHECK-NOT: push 160

    // top -> rec allocates a dynamic frame.
    // CHECK: push 7
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: mod
    // CHECK-NEXT: push [[TOP_REC_CONT:bb[0-9]+]]
    // CHECK-NEXT: jump [[DYN_ALLOC:bb[0-9]+]]
    // CHECK-NEXT: [[DYN_ALLOC]]:
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mload
    // CHECK: push 32
    // CHECK-NEXT: add
    // CHECK-NEXT: mstore
    // CHECK-NEXT: swap1
    // CHECK-NEXT: jump

    // Dynamic returns restore the free-memory and previous frame pointers.
    // CHECK-NEXT: [[TOP_REC_RET:bb[0-9]+]]:
    // CHECK: push [[TOP_AFTER_REC:bb[0-9]+]]
    // CHECK-NEXT: jump [[DYN_EPILOGUE:bb[0-9]+]]
    // CHECK-NEXT: [[DYN_EPILOGUE]]:
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK: push 32
    // CHECK-NEXT: add
    // CHECK-NEXT: mload
    // CHECK: push 160
    // CHECK-NEXT: mstore
    // CHECK-NEXT: jump
    // CHECK-NEXT: [[TOP_M1_RET:bb[0-9]+]]:
    // CHECK: push {{bb[0-9]+}}
    // CHECK-NEXT: jump [[DYN_EPILOGUE]]

    // rec -> rec uses the same dynamic allocator and epilogue.
    // CHECK: push [[PANIC:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push [[REC_RECUR_CONT:bb[0-9]+]]
    // CHECK-NEXT: jump [[DYN_ALLOC]]
    // CHECK-NEXT: [[REC_RECUR_RET:bb[0-9]+]]:
    // CHECK: push [[REC_AFTER_RECUR:bb[0-9]+]]
    // CHECK-NEXT: jump [[DYN_EPILOGUE]]

    // m1 -> m2 is also dynamically allocated.
    // CHECK: push 448
    // CHECK: swap2
    // CHECK-NEXT: pop
    // CHECK-NEXT: pop
    // CHECK-NEXT: push [[PANIC]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push [[M1_M2_CONT:bb[0-9]+]]
    // CHECK-NEXT: jump [[DYN_ALLOC]]
    // CHECK-NEXT: [[M1_M2_RET:bb[0-9]+]]:
    // CHECK: push [[M1_AFTER_M2:bb[0-9]+]]
    // CHECK-NEXT: jump [[DYN_EPILOGUE]]
    // CHECK-NEXT: [[M2_M1_RET:bb[0-9]+]]:
    // CHECK: push [[M2_AFTER_M1:bb[0-9]+]]
    // CHECK-NEXT: jump [[DYN_EPILOGUE]]

    // Tie the top -> rec allocation to its entry and return.
    // CHECK: [[TOP_REC_CONT]]:
    // CHECK: push 576
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: push [[TOP_REC_RET]]
    // CHECK-NEXT: jump [[REC_ENTRY:bb[0-9]+]]
    // CHECK-NEXT: [[REC_ENTRY]]:
    // CHECK-NEXT: push [[REC_BODY:bb[0-9]+]]
    // CHECK-NEXT: jump [[DYN_PROLOGUE:bb[0-9]+]]
    // CHECK: [[REC_BODY]]:

    // Tie the top -> m1 allocation to its entry and return.
    // CHECK: push 544
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: push [[TOP_M1_RET]]
    // CHECK-NEXT: jump [[M1_ENTRY:bb[0-9]+]]
    // CHECK-NEXT: [[M1_ENTRY]]:
    // CHECK: [[TOP_AFTER_REC]]:
    // CHECK: push 5
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: mod
    // CHECK-NEXT: push {{bb[0-9]+}}
    // CHECK-NEXT: jump [[DYN_ALLOC]]

    // Complete the recursive rec call setup.
    // CHECK: [[REC_RECUR_CONT]]:
    // CHECK: push 576
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: push [[REC_RECUR_RET]]
    // CHECK-NEXT: jump [[REC_ENTRY]]
    // CHECK: [[REC_AFTER_RECUR]]:

    // m2 -> m1 uses the allocator too.
    // CHECK: [[M2_M1_CONT:bb[0-9]+]]:
    // CHECK: push 544
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: push [[M2_M1_RET]]
    // CHECK-NEXT: jump [[M1_ENTRY]]
    // CHECK-NEXT: [[M2_BODY:bb[0-9]+]]:
    // CHECK: push 5
    // CHECK: gt
    // CHECK-NEXT: swap1
    // CHECK-NEXT: pop
    // CHECK-NEXT: push [[PANIC]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push [[M2_M1_CONT]]
    // CHECK-NEXT: jump [[DYN_ALLOC]]

    // Complete the m1 -> m2 call setup.
    // CHECK-NEXT: [[M1_M2_CONT]]:
    // CHECK: push 320
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push [[M1_M2_RET]]
    // CHECK-NEXT: push [[M2_BODY]]
    // CHECK-NEXT: jump [[DYN_PROLOGUE]]
    // CHECK: [[M1_AFTER_M2]]:
    // CHECK: [[M2_AFTER_M1]]:
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
