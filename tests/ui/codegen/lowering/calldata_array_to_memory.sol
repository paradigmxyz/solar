//@compile-flags: -Zcodegen --emit=evm-ir-runtime

// A calldata dynamic array converted to memory (declaration initializer,
// assignment, or a struct-literal field) must materialize as a
// `[length][elems...]` copy. Lowering the conversion through the generic
// expression path handed out the raw calldata head word as if it were a
// memory pointer, so the copy read length 0 (a silent miscompile — aave's
// flashloan params are built exactly this way). Runtime behavior is verified
// equal to solc 0.8.30 separately, including empty arrays and >32-byte bytes.

contract C {
    struct P {
        uint256 base;
        uint256[] xs;
        bytes tag;
    }

    uint256 public acc;

    function viaDecl(uint256[] calldata xs) external returns (uint256) {
        uint256[] memory m = xs;
        uint256 s = 0;
        for (uint256 i = 0; i < m.length; i++) {
            s += m[i];
        }
        acc = s;
        return s;
    }

    function viaAssign(uint256[] calldata xs) external pure returns (uint256) {
        uint256[] memory m;
        m = xs;
        return m.length;
    }

    function viaStructLiteral(uint256 base, uint256[] calldata xs, bytes calldata tag)
        external
        pure
        returns (uint256)
    {
        P memory p = P({base: base, xs: xs, tag: tag});
        return p.base + p.xs.length * 10 + p.tag.length * 100;
    }
}
