// Error/event parameters don't shadow type names for subsequent parameters.
// https://github.com/paradigmxyz/solar/issues/219

contract C {
    enum EnumType { A, B, C }

    struct StructType {
        uint x;
    }

    // Parameter named `StructType` should not shadow the type `StructType`.
    error E(EnumType StructType, StructType test);
    
    // Same for events.
    event Ev(EnumType StructType, StructType test);
    
    // And for function type parameters.
    function f(StructType memory a) public pure returns (StructType memory) {
        return a;
    }
}
