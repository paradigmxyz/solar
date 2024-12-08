function funky() {
    uint i;

    while (i++ < 10) continue;

    do ++i; while (i < 20);
    do {} while (i++ < 30);

    for (;;) break;
    for (; i < 40; i++) continue;
    for (; i++ < 50;) continue;

    while (a == 0) uint a = 0; //~ ERROR: unresolved symbol
    //~^ ERROR: variable declaration statements are not allowed as the body of a loop (for, while, do while), meaning they must be inside of a block

    a; //~ ERROR: unresolved symbol
    while (b == 0) { uint b = 0; } //~ ERROR: unresolved symbol
    b; //~ ERROR: unresolved symbol

    do uint c; while (c == 0); //~ ERROR: unresolved symbol
    //~^ ERROR: variable declaration statements are not allowed as the body of a loop (for, while, do while), meaning they must be inside of a block

    c; //~ ERROR: unresolved symbol
    do { uint d; } while (d == 0); //~ ERROR: unresolved symbol
    d; //~ ERROR: unresolved symbol

    for (; false; e++) uint e; //~ ERROR: unresolved symbol
    //~^ ERROR: variable declaration statements are not allowed as the body of a loop (for, while, do while), meaning they must be inside of a block

    e; //~ ERROR: unresolved symbol
    for (; false; f++) { uint f; } //~ ERROR: unresolved symbol
    f; //~ ERROR: unresolved symbol
    for (uint g; false; g++) {
        g;
    }
    g; //~ ERROR: unresolved symbol
}
