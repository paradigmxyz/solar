// Storage size and ABI checks stop at the recursive struct error instead of recursing.

contract Direct {
    struct S { //~ ERROR: recursive struct definition
        S f;
    }
    S s;
}

contract ThroughFixedArray {
    struct S { //~ ERROR: recursive struct definition
        S[2] f;
    }
    S s;
}

contract Mutual {
    struct A { //~ ERROR: recursive struct definition
        B b;
    }
    struct B { //~ ERROR: recursive struct definition
        A a;
    }
    A a;
}

contract Containing {
    struct S { //~ ERROR: recursive struct definition
        S f;
    }
    struct T { //~ ERROR: recursive struct definition
        S s;
    }
    T t;
}

contract InMappingAndArrays {
    struct S { //~ ERROR: recursive struct definition
        S f;
    }
    mapping(uint => S) m;
    S[3] a;
    S[] b;
}

contract PublicVariable {
    struct S { //~ ERROR: recursive struct definition
        S f;
    }
    S public s;
}

contract MemoryParameter {
    struct S { //~ ERROR: recursive struct definition
        S f;
    }
    function f(S memory) public {}
}

contract CalldataParameter {
    struct S { //~ ERROR: recursive struct definition
        S f;
    }
    function f(S calldata) external {}
}

contract StorageParameter {
    struct S { //~ ERROR: recursive struct definition
        S f;
    }
    function f(S storage) internal {}
}

// Recursion through dynamic arrays and mappings is allowed in storage.
contract IndirectStorage {
    struct S {
        S[] f;
        mapping(uint => S) m;
        uint x;
    }
    S s;
    S public p;
    function f(S storage) internal {}
}

contract IndirectMemory {
    struct S {
        S[] f;
    }
    function f(S memory s) public returns (S memory) { //~ ERROR: recursive types cannot be parameter or return types of public functions
    //~^ ERROR: recursive types cannot be parameter or return types of public functions
        return s;
    }
}

contract IndirectCalldata {
    struct S {
        S[] f;
    }
    function f(S calldata) external {} //~ ERROR: recursive types cannot be parameter or return types of public functions
}

contract IndirectPublicGetter {
    struct S {
        S[] f;
    }
    S public s; //~ ERROR: getter must return at least one value
}
