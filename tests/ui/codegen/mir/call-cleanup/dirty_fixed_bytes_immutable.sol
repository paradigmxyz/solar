//@ codegen-matrix: standard
// An immutable stores the raw bits of its initializer, so loading one yields a dirty word that
// typed uses still clean.
//@ run-call: direct => true
//@ run-call: viaCall => true
//@ run-call: viaReturn => true
//@ run-call: viaPointer => true

bytes32 constant WORD = 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff;

function dirty(bytes32 word) pure returns (bytes4 id) {
    assembly { id := word }
}

contract DirtyFixedBytesImmutable {
    bytes4 immutable id;

    constructor() {
        bytes32 word = WORD;
        bytes4 value;
        assembly { value := word }
        id = value;
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

    function viaPointer() external view returns (bool) {
        function(bytes4) internal pure returns (bool) callback = compare;
        return callback(id);
    }
}
