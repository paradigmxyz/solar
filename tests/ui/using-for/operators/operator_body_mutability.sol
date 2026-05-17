//@compile-flags: -Ztypeck

type Int is int16;

using {add as +, neg as -} for Int global;

IAdder constant ADDER = IAdder(address(0)); //~ ERROR: invalid explicit type conversion

function add(Int x, Int y) pure returns (Int) {
    return ADDER.mul(x, y); //~ ERROR: function cannot be declared as `pure`
}

function neg(Int x) pure returns (Int) {
    return ADDER.inc(x); //~ ERROR: function declared as `pure`
}

interface IAdder {
    function mul(Int, Int) external returns (Int);

    function inc(Int) external view returns (Int);
}
