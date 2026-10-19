//@ codegen-matrix: standard
// A `bytesN` parameter keeps a word carrier. Callers clean a dirty argument unless the
// parameter is raw or the callee runs behind an internal function pointer.
//@ run-call: cleaned 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: cleaned 0x12345679ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: raw 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: viaPointer 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: viaPointer 0x12345679ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: dirtyViaPointer 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: dirtyViaPointer 0x12345679ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: viaModifier 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 1
//@ run-call-fail: viaModifier 0x12345679ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: viaPublic 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: isMagic 0x12345678 => true
//@ run-call-fail: 0xafd7207112345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: dirtyReturn 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: dirtyReturn 0x12345679ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: forwarded 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: forwardedRaw 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: operator 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true

type Id is bytes4;

using { eq as == } for Id global;

function eq(Id a, Id b) pure returns (bool) {
    return Id.unwrap(a) == Id.unwrap(b);
}

contract DirtyFixedBytes {
    function dirty(bytes32 word) internal pure returns (bytes4 id) {
        assembly { id := word }
    }

    function cleaned(bytes32 word) external pure returns (bool) {
        return compare(dirty(word));
    }

    function compare(bytes4 id) internal pure returns (bool) {
        return id == 0x12345678;
    }

    function raw(bytes32 word) external pure returns (bytes32) {
        return rawBits(dirty(word));
    }

    function rawBits(bytes4 id) internal pure returns (bytes32 result) {
        assembly { result := id }
    }

    function viaPointer(bytes32 word) external pure returns (bool) {
        function(bytes4) internal pure returns (bool) callback = compare;
        return callback(dirty(word));
    }

    function dirtyViaPointer(bytes32 word) external pure returns (bool) {
        function(bytes32) internal pure returns (bytes4) callback = dirty;
        return callback(word) == 0x12345678;
    }

    modifier onlyMagic(bytes4 id) {
        require(id == 0x12345678);
        _;
    }

    function viaModifier(bytes32 word) external pure onlyMagic(dirty(word)) returns (uint256) {
        return 1;
    }

    // Reached with dirty arguments from `viaPublic` and with calldata that the ABI decoder
    // validates.
    function isMagic(bytes4 id) public pure returns (bool) {
        return id == 0x12345678;
    }

    function viaPublic(bytes32 word) external pure returns (bool) {
        return isMagic(dirty(word));
    }

    function dirtyReturn(bytes32 word) external pure returns (bool) {
        return dirty(word) == 0x12345678;
    }

    function forwarded(bytes32 word) external pure returns (bool) {
        return compare(identity(dirty(word)));
    }

    function identity(bytes4 id) internal pure returns (bytes4) {
        return id;
    }

    function forwardedRaw(bytes32 word) external pure returns (bytes32) {
        return rawBits(identity(dirty(word)));
    }

    function operator(bytes32 word) external pure returns (bool) {
        Id id;
        bytes4 value = dirty(word);
        assembly { id := value }
        return id == Id.wrap(0x12345678);
    }
}
