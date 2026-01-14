struct S {
    uint x;
}

uint memory constant a0 = 0;    //~ ERROR: data locations are not allowed here
uint[] memory constant b0 = []; //~ ERROR: data locations are not allowed here
S memory constant c0 = S(0);    //~ ERROR: data locations are not allowed here
S[] memory constant d0 = [];    //~ ERROR: data locations are not allowed here
