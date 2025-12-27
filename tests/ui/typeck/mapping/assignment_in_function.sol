//@compile-flags: -Ztypeck

contract F {
    mapping (uint => uint) a;
    mapping (uint => uint) b;

    function foo() public {
        a = b; //~ ERROR: Types in storage containing (nested) mappings cannot be assigned to.
    }
}

contract LocalStorageOK {
    mapping (uint => uint) a;
    mapping (uint => uint) b;

    function foo() public view {
        mapping (uint => uint) storage c = b;  // OK - local storage pointer
        b = c; //~ ERROR: Types in storage containing (nested) mappings cannot be assigned to.
    }
}
