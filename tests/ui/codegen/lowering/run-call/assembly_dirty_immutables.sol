//@ codegen-matrix: standard
//@ run-call: id => 0xffffffff
//@ run-call: flag => true
//@ run-call: owner => 0xffffffffffffffffffffffffffffffffffffffff
//@ run-call: small => 255
//@ run-call: wrapped => 0xffffffff
//@ run-call: unwrapped => true

// A public getter never returns the dirty bits of an assembly-written immutable, and a typed use
// of one cleans it.
type Wrapped is bytes4;

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
