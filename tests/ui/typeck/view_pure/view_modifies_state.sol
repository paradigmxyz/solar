//@compile-flags: -Ztypeck
// Test: view function cannot modify state variables

contract C {
    uint256 public x;

    function viewModifiesState() public view {
        x = 42;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }
}
