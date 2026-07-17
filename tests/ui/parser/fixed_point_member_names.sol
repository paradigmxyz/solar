//@ compile-flags: --stop-after parsing

contract C {
    function f() internal pure {
        this.fixed128x18; //~ ERROR: expected identifier
        f().ufixed128x18; //~ ERROR: expected identifier
    }
}
