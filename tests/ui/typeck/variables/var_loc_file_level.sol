struct S {
    uint x;
}

uint memory constant a0 = 0;    //~ ERROR: data locations are not allowed here
uint[] memory constant b0 = []; //~ ERROR: data locations are not allowed here
//~^ ERROR: only constants of value type and byte array type are implemented
//~| ERROR: cannot infer array element type
S memory constant c0 = S(0);    //~ ERROR: data locations are not allowed here
//~^ ERROR: only constants of value type and byte array type are implemented
S[] memory constant d0 = [];    //~ ERROR: data locations are not allowed here
//~^ ERROR: only constants of value type and byte array type are implemented
//~| ERROR: cannot infer array element type
