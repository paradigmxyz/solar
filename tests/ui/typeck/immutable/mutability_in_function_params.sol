contract C {
    function f(
        uint constant a, //~ ERROR: mutability is not allowed here
        uint immutable b //~ ERROR: mutability is not allowed here
    ) public returns(
        uint constant c, //~ ERROR: mutability is not allowed here
        uint immutable d //~ ERROR: mutability is not allowed here
    ) {}
}
