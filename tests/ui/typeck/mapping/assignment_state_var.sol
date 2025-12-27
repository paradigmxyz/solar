//@compile-flags: -Ztypeck

contract C {
    mapping (uint => address payable []) public a = a; //~ ERROR: Types in storage containing (nested) mappings cannot be assigned to.
}

contract D {
    mapping (uint => uint) a;
    mapping (uint => uint) b = a; //~ ERROR: Types in storage containing (nested) mappings cannot be assigned to.
}
