//@ codegen-matrix: standard
//@ run-call: raw => 0x1234567800000000000000000000000000000000000000000000000000000000

// A modifier parameter sees the cleaned `bytes4` that the assembly block of `dirty` assigned.
contract AssemblyDirtyModifierArgument {
    bytes32 internal seen;

    modifier record(bytes4 id) {
        bytes32 word;
        assembly { word := id }
        seen = word;
        _;
    }

    function raw() external record(dirty()) returns (bytes32) {
        return seen;
    }

    function dirty() internal pure returns (bytes4 id) {
        assembly { id := 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff }
    }
}
