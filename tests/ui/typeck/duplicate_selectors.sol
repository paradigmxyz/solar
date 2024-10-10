contract C {
    function mintEfficientN2M_001Z5BWH() public {}
}

contract D is C {
    //~^ ERROR function signature hash collision
    function BlazingIt4490597615() public {}
}
