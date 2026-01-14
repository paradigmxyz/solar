//@ compile-flags: -Ztypeck
interface I {}
contract C {
    function f() public {
        new I(); //~ ERROR: cannot instantiate
    }
}
