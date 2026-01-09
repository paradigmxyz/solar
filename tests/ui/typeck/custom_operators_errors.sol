//@compile-flags: -Ztypeck

type Int is int128;
type Other is int128;

function add(Int x, Int y) pure returns (Int) {}
function neg(Int x) pure returns (Int) {}

using {add as +, neg as -} for Int global;

function testMismatchedTypes(Int a, Other b) pure returns (Int) {
    return a + b; //~ ERROR: cannot apply operator `+` to `Int` and `Other`
}

function testNoOperator(Int a, Int b) pure returns (Int) {
    return a * b; //~ ERROR: cannot apply operator `*` to `Int` and `Int`
}

function testUnaryNoOperator(Int a) pure returns (Int) {
    return ~a; //~ ERROR: cannot apply unary operator `~` to `Int`
}
