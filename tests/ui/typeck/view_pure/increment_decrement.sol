//@compile-flags: -Ztypeck
// Test: increment/decrement on state variables modifies state

contract C {
    uint256 x;

    function incrementInView() public view {
        x++;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function decrementInView() public view {
        x--;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function preIncrementInView() public view {
        ++x;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function preDecrementInView() public view {
        --x;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function incrementInPure() public pure {
        uint local = 5;
        local++;
        ++local;
    }
}
