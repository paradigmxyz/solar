//@ codegen-matrix: standard
//@ run-call: id => 0xffffffff
//@ run-call: flag => true
//@ run-call: owner => 0xffffffffffffffffffffffffffffffffffffffff
//@ run-call: small => 255

// Like solc, an immutable stores its value cleaned, so a public getter never returns the dirty
// bits of an assembly-written initializer.
contract AssemblyDirtyImmutables {
    bytes4 public immutable id;
    bool public immutable flag;
    address public immutable owner;
    uint8 public immutable small;

    constructor() {
        bytes4 rawId;
        bool rawFlag;
        address rawOwner;
        uint8 rawSmall;
        assembly {
            rawId := not(0)
            rawFlag := 0x100
            rawOwner := not(0)
            rawSmall := 0x1ff
        }
        id = rawId;
        flag = rawFlag;
        owner = rawOwner;
        small = rawSmall;
    }
}
