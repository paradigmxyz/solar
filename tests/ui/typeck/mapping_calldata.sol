//@compile-flags: -Ztypeck

contract C {
    struct WithMapping {
        mapping(uint => uint) m;
    }

    // Calldata variable containing mapping is invalid
    function f(WithMapping calldata w) internal {} //~ ERROR: is only valid in storage because it contains a (nested) mapping
}
