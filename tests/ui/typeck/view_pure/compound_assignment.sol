//@compile-flags: -Ztypeck
// Test: compound assignment on state variables modifies state

contract C {
    uint256 x;

    function addAssignInView() public view {
        x += 1;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function subAssignInView() public view {
        x -= 1;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function mulAssignInView() public view {
        x *= 2;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function divAssignInView() public view {
        x /= 2;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function modAssignInView() public view {
        x %= 2;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function compoundInPure() public pure {
        uint local = 5;
        local += 1;
        local -= 1;
    }
}
