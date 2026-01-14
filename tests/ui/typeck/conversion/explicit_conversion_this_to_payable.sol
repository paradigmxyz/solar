//@compile-flags: -Ztypeck
contract C {
    function f() public pure {
        address payable p = payable(this); //~ ERROR: invalid explicit type conversion
        address payable q = payable(address(this));
    }
}
