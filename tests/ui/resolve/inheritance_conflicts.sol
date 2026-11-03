contract A {
    uint public x = 0; //~ ERROR: cannot override non-virtual function
    //~^ ERROR: cannot override public state variable
}

contract B is A {
    uint public x = 1; //~ ERROR: identifier `x` already declared
    //~^ ERROR: overriding public state variable is missing `override` specifier
    //~| ERROR: overriding public state variable is missing `override` specifier
}

contract AA {
    uint public y = 2;
}

contract BB {
    uint public y = 3; //~ ERROR: identifier `y` already declared
}

contract CC is AA, BB {} //~ ERROR: derived contract must override function `y`

contract AAA {
    uint public z = 4;
}

contract BBB {
    uint public z = 5; //~ ERROR: identifier `z` already declared
}

contract CCC is BBB, AAA {} //~ ERROR: derived contract must override function `z`
