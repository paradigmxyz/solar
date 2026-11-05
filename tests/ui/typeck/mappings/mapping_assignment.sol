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

contract TupleAssignment {
    mapping(uint => uint) a;
    mapping(uint => uint) b;
    uint x;
    uint y;

    function tupleAssign() public {
        (a, x) = (b, y); //~ ERROR: types in storage containing (nested) mappings cannot be assigned to
    }
}

// A struct that reaches its mapping only through its own recursion still
// contains one: mapping discovery must descend through the cycle instead of
// giving up on recursive structs.
contract RecursiveStructWithMapping {
    struct S {
        uint256 x;
        mapping(uint256 => S) recurse;
    }

    S a;
    S[3] b;

    function assignRecursive() public {
        b[0] = a; //~ ERROR: types in storage containing (nested) mappings cannot be assigned to
    }
}
