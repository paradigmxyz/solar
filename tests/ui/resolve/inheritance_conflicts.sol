contract A {
    uint public x = 0;
}

contract B is A {
    uint public x = 1; //~ ERROR identifier `x` already declared
}

contract AA {
    uint public y = 2;
}

contract BB {
    uint public y = 3; //~ ERROR identifier `y` already declared
}

contract CC is AA, BB {}
