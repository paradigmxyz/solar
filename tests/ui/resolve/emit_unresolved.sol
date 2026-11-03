contract C {
    function f() public {
        emit Missing(); //~ ERROR: unresolved symbol `Missing`
        revert Missing(); //~ ERROR: unresolved symbol `Missing`
        emit Missing.E(); //~ ERROR: unresolved symbol `Missing`
        revert Missing.Er(); //~ ERROR: unresolved symbol `Missing`
    }
}
