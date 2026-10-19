//@ codegen-matrix: standard
// A base constructor argument, written in the inheritance list or in the constructor, reaches the
// constructor parameter's raw binding, so a value that assembly retyped through an internal
// function pointer keeps its raw bits.
//@ run-call: seenA => 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: seenB => 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff

bytes32 constant WORD = 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff;

function identity(bytes32 word) pure returns (bytes32) {
    return word;
}

function retyped() pure returns (bytes4) {
    function(bytes32) internal pure returns (bytes32) typed = identity;
    function(bytes32) internal pure returns (bytes4) raw;
    assembly { raw := typed }
    return raw(WORD);
}

contract BaseA {
    bytes32 internal wordA;

    constructor(bytes4 id) {
        bytes32 raw;
        assembly { raw := id }
        wordA = raw;
    }
}

contract BaseB {
    bytes32 internal wordB;

    constructor(bytes4 id) {
        bytes32 raw;
        assembly { raw := id }
        wordB = raw;
    }
}

contract DirtyFixedBytesBaseArguments is BaseA(retyped()), BaseB {
    constructor() BaseB(retyped()) {}

    function seenA() external view returns (bytes32) {
        return wordA;
    }

    function seenB() external view returns (bytes32) {
        return wordB;
    }
}
