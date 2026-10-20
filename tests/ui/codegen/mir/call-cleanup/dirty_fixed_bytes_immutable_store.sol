//@ codegen-matrix: standard
// A dirty word that reaches the store of an immutable without a copy edge, here the result of a
// call through a retyped pointer, is cleaned at the store.
//@ run-call: direct => true
//@ run-call: viaCall => true
//@ run-call: viaReturn => true

bytes32 constant WORD = 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff;

function identity(bytes32 word) pure returns (bytes32) {
    return word;
}

contract DirtyFixedBytesImmutableStore {
    bytes4 immutable id;

    constructor() {
        function(bytes32) internal pure returns (bytes32) typed = identity;
        function(bytes32) internal pure returns (bytes4) retyped;
        assembly { retyped := typed }
        id = retyped(WORD);
    }

    function direct() external view returns (bool) {
        return id == 0x12345678;
    }

    function viaCall() external view returns (bool) {
        return compare(id);
    }

    function compare(bytes4 value) internal pure returns (bool) {
        return value == 0x12345678;
    }

    function viaReturn() external view returns (bool) {
        return get() == 0x12345678;
    }

    function get() internal view returns (bytes4) {
        return id;
    }
}
