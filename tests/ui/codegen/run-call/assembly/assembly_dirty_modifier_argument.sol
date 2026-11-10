//@ codegen-matrix: standard
//@ run-call: raw => 0x1234567800000000000000000000000000000000000000000000000000000000

// A modifier argument is cleaned to the parameter's type before assembly reads the parameter.
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
