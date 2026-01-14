contract C {
    function f() public pure {
        super(this).f();
    }
}
