function funky() {
    uint i;

    while (i++ < 10) continue;

    do ++i; while (i < 20);
    do {} while (i++ < 30);

    for (;;) break;
    for (; i < 40; i++) continue;
    for (; i++ < 50;) continue;

    // ---
    // TODO: `Error: Variable declarations can only be used inside blocks.`

    while (a == 0) uint a = 0; //~ ERROR: unresolved symbol
    a; //~ ERROR: unresolved symbol
    while (b == 0) { uint b = 0; } //~ ERROR: unresolved symbol
    b; //~ ERROR: unresolved symbol

    do uint c; while (c == 0); //~ ERROR: unresolved symbol
    c; //~ ERROR: unresolved symbol
    do { uint d; } while (d == 0); //~ ERROR: unresolved symbol
    d; //~ ERROR: unresolved symbol

    for (; false; e++) uint e; //~ ERROR: unresolved symbol
    e; //~ ERROR: unresolved symbol
    for (; false; f++) { uint f; } //~ ERROR: unresolved symbol
    f; //~ ERROR: unresolved symbol
    for (uint g; false; g++) {
        g;
    }
    g; //~ ERROR: unresolved symbol
}
