//@ revisions: hashes abi
//@[hashes] compile-flags: --emit=hashes
//@[abi] compile-flags: --emit=abi

library Invalid {
    struct Node { Node[] children; }
    struct Bad { Bad[] children; function() internal callback; }
    struct Inner { function() internal callback; }
    struct Outer { Inner[] entries; }

    function memoryParameter(Node memory) public {} //~ ERROR: recursive types cannot be parameter or return types of public functions
    function memoryReturn() external returns (Node memory) {} //~ ERROR: recursive types cannot be parameter or return types of public functions
    function recursiveInternal(Bad storage) public {} //~ ERROR: types containing internal function pointers cannot be parameter or return types of public functions
    function nestedInternal(Outer storage) external {} //~ ERROR: types containing internal function pointers cannot be parameter or return types of public functions
    function internalParameter(function() internal) public {} //~ ERROR: types containing internal function pointers cannot be parameter or return types of public functions
    function internalReturn() external returns (function() internal) {} //~ ERROR: types containing internal function pointers cannot be parameter or return types of public functions
}
