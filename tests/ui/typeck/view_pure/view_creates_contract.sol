//@compile-flags: -Ztypeck
// Test: view function cannot create contracts

contract Other {}

contract C {
    function viewCreatesContract() public view returns (Other) {
        return new Other();
        //~^ ERROR: not yet implemented
        //~| ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }
}
