//@compile-flags: -Ztypeck

contract H {
    struct S { mapping (uint => uint) a; }

    S x;
    S y = x; //~ ERROR: Types in storage containing (nested) mappings cannot be assigned to.
    S z = z; //~ ERROR: Types in storage containing (nested) mappings cannot be assigned to.
}
