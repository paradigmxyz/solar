//@compile-flags: -Ztypeck

contract Helper {
    mapping(uint => uint) m;
}

contract C {
    mapping(uint => uint) m1;       // OK - no initializer
    mapping(uint => uint) m2;       // OK - no initializer

    struct S {
        uint x;
    }

    S s1;                           // OK - no initializer
    S s2;                           // OK - no initializer

    // Note: Mappings cannot be initialized in Solidity syntax at all.
    // The check is defensive - if a type contains mapping and has initializer,
    // we catch it at the semantic level.
}
