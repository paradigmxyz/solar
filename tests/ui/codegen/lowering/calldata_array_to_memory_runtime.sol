//@ codegen-matrix: standard
//@ run-call: viaStructLiteral 7, [1, 2, 3], 0x4142 => 237
//@ run-call: viaStructLiteral 5, [], 0x => 5
//@ run-call: viaStructLiteral 0, [9], 0x414243 => 310
//@ run-call: viaDeclaration [1, 2, 3, 4] => 4
//@ run-call: viaDeclaration [] => 0
//@ run-call: viaAssignment [1, 2] => 2
//@ run-call: viaAssignment [] => 0

// Runtime cover for the lengths the IR test pins. A calldata array converted
// to memory keeps its length through the copy, and reading that length back
// must give the calldata one whether or not the read survives optimization.
contract C {
    struct P {
        uint256 base;
        uint256[] xs;
        bytes tag;
    }

    function viaStructLiteral(uint256 base, uint256[] calldata xs, bytes calldata tag)
        external
        pure
        returns (uint256)
    {
        P memory p = P({base: base, xs: xs, tag: tag});
        return p.base + p.xs.length * 10 + p.tag.length * 100;
    }

    function viaDeclaration(uint256[] calldata xs) external pure returns (uint256) {
        uint256[] memory m = xs;
        return m.length;
    }

    function viaAssignment(uint256[] calldata xs) external pure returns (uint256) {
        uint256[] memory m;
        m = xs;
        return m.length;
    }
}
