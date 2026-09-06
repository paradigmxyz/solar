//@compile-flags: --libraries L=0x1000000000000000000000000000000000000001 -Zdump=evm-ir-runtime
//@ filecheck:

// A linked library call whose struct parameter carries dynamic fields:
// the head word of each dynamic field holds an args-relative offset and the
// `[len][data...]` tail travels after the heads; the library wrapper decodes
// the tail into fresh callee memory (a raw word would be a caller-memory
// pointer, meaningless across the delegatecall boundary — this shape is
// aave's FlashloanParams). Runtime behavior is verified equal to solc
// 0.8.30's linked flow separately, including empty and multi-word tails.

library L {
    struct P {
        uint256 base;
        uint256[] xs;
        bytes tag;
        address who;
    }

    // CHECK-LABEL: @module L_runtime
    // CHECK: push 0xfa06cb96
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[L_REJECT:bb[0-9]+]], [[L_GUARD:bb[0-9]+]]
    // CHECK-NEXT: [[L_GUARD]]:
    // CHECK-NEXT: push_immutable {{[0-9]+}}, 20
    // CHECK-NEXT: address
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[L_REJECT]], [[APPLY:bb[0-9]+]]
    // CHECK-NEXT: [[APPLY]]:
    // CHECK: calldataload
    // CHECK: calldataload
    // CHECK: calldatacopy
    // CHECK: calldatacopy
    // CHECK: keccak256
    // CHECK: sstore
    // CHECK: return
    function apply_(mapping(address => uint256) storage m, P memory p)
        public
        returns (uint256)
    {
        uint256 acc = p.base;
        for (uint256 i = 0; i < p.xs.length; i++) {
            acc += p.xs[i] * (i + 1);
        }
        acc += p.tag.length * 1000;
        m[p.who] = acc;
        return acc;
    }
}

contract C {
    // CHECK-LABEL: @module C_runtime
    // CHECK: push 0x2220ae27
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[GO:bb[0-9]+]], [[SCORE_CHECK:bb[0-9]+]]
    // CHECK-NEXT: [[SCORE_CHECK]]:
    // CHECK-NEXT: push 0x776f3843
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi {{bb[0-9]+}}, [[SCORE:bb[0-9]+]]
    // CHECK-NEXT: [[SCORE]]:
    // CHECK: keccak256
    // CHECK-NEXT: sload
    // CHECK: return
    // CHECK: jump [[COPY_TAG:bb[0-9]+]]
    // CHECK-NEXT: [[COPY_TAG]]:
    // CHECK: mcopy
    // CHECK: push 0x1000000000000000000000000000000000000001
    // CHECK: delegatecall
    // CHECK-NEXT: jumpi {{bb[0-9]+}}, [[FAIL:bb[0-9]+]]
    // CHECK-NEXT: [[FAIL]]{{( \[cold\])?}}:
    // CHECK: returndatacopy
    // CHECK: revert
    mapping(address => uint256) public score;

    // CHECK: calldatacopy
    // CHECK: calldatacopy
    // CHECK: push 0x7d0365cb
    // CHECK-NEXT: push 225
    // CHECK-NEXT: shl
    // CHECK: mcopy
    // CHECK: jumpi {{bb[0-9]+}}, [[COPY_TAG]]
    // CHECK-NEXT: [[GO]]:
    // CHECK: calldatasize
    function go(uint256 base, uint256[] calldata xs, bytes calldata tag, address who)
        external
        returns (uint256)
    {
        uint256[] memory mxs = xs;
        bytes memory mtag = tag;
        return L.apply_(score, L.P({base: base, xs: mxs, tag: mtag, who: who}));
    }
}
