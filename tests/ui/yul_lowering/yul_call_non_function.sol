//@compile-flags: -Ztypeck

contract C {
    function f(uint256 dialect_helper) public {
        assembly {
            dialect_helper(1) //~ ERROR: expected function
        }
    }
}
