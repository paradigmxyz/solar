//@compile-flags: -Ztypeck
// Test: view function cannot emit events

contract C {
    event MyEvent(uint256 value);

    function viewEmitsEvent() public view {
        emit MyEvent(42);
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }
}
