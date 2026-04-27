//@check-pass
// Events and errors can shadow builtin identifiers (this, super).
// This is allowed because they exist in different namespaces and cannot be confused.

contract C {
    event this();
    event super();
}

contract D {
    error this();
    error super();
}

contract E {
    // Can use both event and function with builtin names
    event this();
    function f() public {
        address x = address(this);
    }
}
