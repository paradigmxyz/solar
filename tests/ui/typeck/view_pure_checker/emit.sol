library L {
    event E(uint256);
}

contract C {
    event E(uint256);

    function f() public pure {
        emit E(1);
        //~^ ERROR: function cannot be declared as pure because this expression (potentially) modifies the state
    }

    function qualified() public pure {
        emit L.E(1);
        //~^ ERROR: function cannot be declared as pure because this expression (potentially) modifies the state
    }
}
