//@compile-flags: -Zcodegen --libraries L=0x1000000000000000000000000000000000000001 --emit=bin-runtime

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
    mapping(address => uint256) public score;

    function go(uint256 base, uint256[] calldata xs, bytes calldata tag, address who)
        external
        returns (uint256)
    {
        uint256[] memory mxs = xs;
        bytes memory mtag = tag;
        return L.apply_(score, L.P({base: base, xs: mxs, tag: mtag, who: who}));
    }
}
