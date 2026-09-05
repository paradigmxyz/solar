//@ codegen-matrix: standard
//@ run-call: run (0, [], 0x) => 0
//@ run-call: run (0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, [11, 13], 0x000102) => 4

// Decoding the dynamic fields consumes unique copy and store operands while
// retaining the struct pointer and the other decoded fields.
contract DeadMultiOperands {
    struct P {
        uint256 base;
        uint256[] xs;
        bytes tag;
    }

    function run(P memory p) public pure returns (uint256) {
        unchecked {
            return p.base + p.xs.length + p.tag.length;
        }
    }
}
