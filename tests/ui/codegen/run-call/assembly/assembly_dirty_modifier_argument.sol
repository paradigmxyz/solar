//@ codegen-matrix: standard
//@ run-call: raw => 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff

// A modifier parameter that assembly reads keeps the bits of its argument, as a function
// parameter does.
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
