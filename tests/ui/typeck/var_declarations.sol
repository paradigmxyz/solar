//@compile-flags: -Ztypeck

// Test variable declaration rules

// Valid file-level constant
uint constant C = 100;

contract Test {
    // Struct with mapping
    struct S { mapping(uint => uint) m; }

    // Valid declarations
    uint constant VALID_CONST = 100;
    uint immutable i;
    mapping(uint => uint) m;

    function f() public {
        // Mapping in memory is invalid
        mapping(uint => uint) memory badMap; //~ ERROR: is only valid in storage because it contains a (nested) mapping

        // Struct with mapping in memory is invalid
        S memory s; //~ ERROR: is only valid in storage because it contains a (nested) mapping
    }
}
