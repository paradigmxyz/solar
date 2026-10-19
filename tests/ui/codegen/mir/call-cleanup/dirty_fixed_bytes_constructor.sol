//@ codegen-matrix: standard
// Calls outside function bodies feed the raw-scalar analysis too: state variable initializers,
// base constructor arguments, and modifier arguments.
//@ run-call: viaPointer => true
//@ run-call: forwardedRaw => 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: baseRaw => 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: modifierRaw => 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: cleanedBase => true

bytes32 constant WORD = 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff;

function dirty(bytes32 word) pure returns (bytes4 id) {
    assembly { id := word }
}

function identity(bytes4 id) pure returns (bytes4) {
    return id;
}

function rawBits(bytes4 id) pure returns (bytes32 result) {
    assembly { result := id }
}

function compare(bytes4 id) pure returns (bool) {
    return id == 0x12345678;
}

contract Base {
    bytes32 internal baseWord;
    bool internal baseCleaned;

    constructor(bytes4 id, bytes4 other) {
        bytes32 word;
        assembly { word := id }
        baseWord = word;
        baseCleaned = compare(other);
    }
}

contract DirtyFixedBytesConstructor is Base(identity(dirty(WORD)), dirty(WORD)) {
    bool public viaPointer = pointer(dirty(WORD));
    bytes32 public forwardedRaw = rawBits(identity(dirty(WORD)));
    bytes32 public modifierRaw;

    modifier record(bytes4 id) {
        bytes32 word;
        assembly { word := id }
        modifierRaw = word;
        _;
    }

    constructor() record(dirty(WORD)) {}

    function baseRaw() external view returns (bytes32) {
        return baseWord;
    }

    function cleanedBase() external view returns (bool) {
        return baseCleaned;
    }

    function pointer(bytes4 id) internal pure returns (bool) {
        function(bytes4) internal pure returns (bool) callback = compare;
        return callback(id);
    }
}
