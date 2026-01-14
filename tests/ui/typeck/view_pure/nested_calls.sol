//@compile-flags: -Ztypeck
// Test: nested function calls respecting mutability

contract C {
    uint256 public x;

    function pureFunc() public pure returns (uint256) {
        return 42;
    }

    function viewFunc() public view returns (uint256) {
        return x;
    }

    // OK: view calling pure (but warns it could be pure)
    function viewCallsPure() public view returns (uint256) {
    //~^ WARN: function state mutability can be restricted to pure
        return pureFunc();
        //~^ ERROR: not yet implemented
    }

    // ERROR: pure calling view
    function pureCallsView() public pure returns (uint256) {
        return viewFunc();
        //~^ ERROR: not yet implemented
        //~| ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires "view"
    }
}
