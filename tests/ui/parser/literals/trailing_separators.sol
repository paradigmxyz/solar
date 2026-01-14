struct S {
    uint x
} //~ ERROR: expected `;`

enum E {
    V, //~ ERROR: trailing `,` separator is not allowed
}

function f(E arg,) { //~ ERROR: trailing `,` separator is not allowed
    arg;
}
