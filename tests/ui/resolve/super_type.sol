contract A {
    struct S1 {
        uint256 x;
    }
}

contract B {
    struct S2 {
        uint256 x;
    }
}

contract C is A, B {
    function f() public {
        this.S1 memory x0; //~ ERROR: `this` is a builtin, which cannot be indexed in type paths
        super.S1 memory x1; //~ ERROR: `super` is a builtin, which cannot be indexed in type paths
        super.S2 memory x2; //~ ERROR: `super` is a builtin, which cannot be indexed in type paths
        super.super.S2 memory x3; //~ ERROR: `super` is a builtin, which cannot be indexed in type paths
    }
}

contract D is this.C {} //~ ERROR: unresolved symbol `this`
contract E is super.C {} //~ ERROR: unresolved symbol `super`

contract F {
    struct this { uint x; } //~ ERROR: the name `this` is reserved
    struct super { uint x; } //~ ERROR: the name `super` is reserved
}
