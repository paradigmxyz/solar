// `new` on a contract that still has unimplemented members (solc 3656 on the
// definition) must also fail at the use site, so creation typing does not
// proceed for a non-deployable type.

contract Abs { //~ ERROR: contract `Abs` has unimplemented functions
    function f() public virtual;
}

contract User {
    function g() public {
        new Abs(); //~ ERROR: cannot instantiate an abstract contract
    }
}
