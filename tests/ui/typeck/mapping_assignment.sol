//@compile-flags: -Ztypeck

contract C {
    mapping(uint => uint) a;
    mapping(uint => uint) b;

    function assignMappings() public {
        a = b; //~ ERROR: types in storage containing (nested) mappings cannot be assigned to
    }
}

contract LocalStoragePointer {
    mapping(uint => uint) a;
    mapping(uint => uint) b;

    function localPointer() public view {
        mapping(uint => uint) storage c = b; // OK - local storage pointer.
        b = c; //~ ERROR: types in storage containing (nested) mappings cannot be assigned to
    }
}

contract StructWithMapping {
    struct S {
        mapping(uint => uint) m;
    }

    S x;
    S y;

    function assignStructs() public {
        x = y; //~ ERROR: types in storage containing (nested) mappings cannot be assigned to
    }
}
