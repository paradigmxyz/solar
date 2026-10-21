//@ codegen-matrix: standard
//@ run-call: id => 0xffffffff
//@ run-call: flag => true
//@ run-call: owner => 0xffffffffffffffffffffffffffffffffffffffff
//@ run-call: small => 255
//@ run-call: wrapped => 0xffffffff
//@ run-call: unwrapped => true
//@ run-call: Stored::stored => 0xffffffff
//@ run-call: Stored::storedWrapped => 0xffffffff
//@ run-call: Loop::looped => 0xffffffff

// A public getter never returns the dirty bits of an assembly-written immutable, and a typed use
// of one cleans it.
type Wrapped is bytes4;

function full(uint256) pure returns (bytes32) {
    return bytes32(type(uint256).max);
}

contract AssemblyDirtyImmutables {
    bytes4 public immutable id;
    bool public immutable flag;
    address public immutable owner;
    uint8 public immutable small;
    Wrapped public immutable wrapped;

    constructor() {
        bytes4 rawId;
        bool rawFlag;
        address rawOwner;
        uint8 rawSmall;
        Wrapped rawWrapped;
        assembly {
            rawId := not(0)
            rawFlag := 0x100
            rawOwner := not(0)
            rawSmall := 0x1ff
            rawWrapped := not(0)
        }
        id = rawId;
        flag = rawFlag;
        owner = rawOwner;
        small = rawSmall;
        wrapped = rawWrapped;
    }

    function unwrapped() external view returns (bool) {
        return Wrapped.unwrap(wrapped) == bytes4(0xffffffff);
    }
}

// The dirty word comes from a call through a retyped pointer, so no copy edge ties it to the
// immutable; the store cleans it.
contract Stored {
    bytes4 public immutable stored;
    Wrapped public immutable storedWrapped;

    constructor() {
        function(uint256) internal pure returns (bytes32) typed = full;
        function(uint256) internal pure returns (bytes4) retyped;
        function(uint256) internal pure returns (Wrapped) retypedWrapped;
        assembly {
            retyped := typed
            retypedWrapped := typed
        }
        stored = retyped(0);
        storedWrapped = retypedWrapped(0);
    }
}

// The dirty word crosses loop phis before the store; the clean does not rely on a dirty mark.
contract Loop {
    bytes4 public immutable looped;

    constructor() {
        function(uint256) internal pure returns (bytes32) typed = full;
        function(uint256) internal pure returns (bytes4) retyped;
        assembly { retyped := typed }
        bytes4 a = retyped(0);
        bytes4 b = a;
        bytes4 c = b;
        bytes4 d = c;
        for (uint256 i; i < 3; ++i) {
            d = c;
            c = b;
            b = a;
            a = d;
        }
        looped = a;
    }
}
