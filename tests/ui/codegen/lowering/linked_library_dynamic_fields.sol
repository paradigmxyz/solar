//@compile-flags: -Zcodegen --libraries L=0x1000000000000000000000000000000000000001 -Zdump=evm-ir-runtime
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

    // CHECK-LABEL: @module runtime
    // CHECK: push 0xfa06cb96
    // CHECK: eq
    // CHECK-NEXT: push [[APPLY:bb[0-9]+]]
    // CHECK: [[APPLY]]:
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
    // CHECK-LABEL: @module runtime
    // CHECK: push 0x2220ae27
    // CHECK: eq
    // CHECK-NEXT: push [[GO:bb[0-9]+]]
    // CHECK: push 0x776f3843
    // CHECK: eq
    // CHECK-NEXT: push [[SCORE:bb[0-9]+]]
    // CHECK: [[SCORE]]:
    // CHECK: keccak256
    // CHECK-NEXT: sload
    // CHECK: return
    mapping(address => uint256) public score;

    // CHECK: [[GO]]:
    // CHECK: calldatacopy
    // CHECK: calldatacopy
    // CHECK: push 0xfa06cb96
    // CHECK: mcopy
    // CHECK: mcopy
    // CHECK: push 0x1000000000000000000000000000000000000001
    // CHECK: delegatecall
    // CHECK: returndatacopy
    // CHECK: revert
    function go(uint256 base, uint256[] calldata xs, bytes calldata tag, address who)
        external
        returns (uint256)
    {
        uint256[] memory mxs = xs;
        bytes memory mtag = tag;
        return L.apply_(score, L.P({base: base, xs: mxs, tag: mtag, who: who}));
    }
}
