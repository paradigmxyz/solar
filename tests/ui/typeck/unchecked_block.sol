function f() {
    unchecked {}

    unchecked {
        unchecked {} //~ ERROR: `unchecked` blocks cannot be nested
        unchecked {} //~ ERROR: `unchecked` blocks cannot be nested
    }
}

contract C {
    function f() public {
        unchecked {}

        unchecked {
            unchecked {} //~ ERROR: `unchecked` blocks cannot be nested
            unchecked {} //~ ERROR: `unchecked` blocks cannot be nested
        }
    }
}
