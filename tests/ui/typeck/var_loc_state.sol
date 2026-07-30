struct S {
    uint x;
}

contract C {
    uint memory a1 = 0;    //~ ERROR: data location can only be specified for array, struct or mapping types
    uint[] memory b1 = []; //~ ERROR: invalid data location
    S memory c1 = S(0);    //~ ERROR: invalid data location
    S[] memory d1 = [];    //~ ERROR: invalid data location
}
