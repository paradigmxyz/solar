contract C {
    struct S {
        uint256 value;
    }

    mapping(uint256 => S) values;
    uint256[] array;
    uint256 state;

    function writes() public view {
        values[0].value = 1;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
        delete array[0];
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
        state++;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function tupleWrite() public view {
        (state, array[0]) = (1, 2);
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
        //~| ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }
}
