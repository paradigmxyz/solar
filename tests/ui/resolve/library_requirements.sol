library A{}

library B is A {} //~ERROR: library is not allowed to inherit

library C {
    uint256 constant x = 1;
    uint256 y; //~ERROR: library cannot have non-constant state variable
}
