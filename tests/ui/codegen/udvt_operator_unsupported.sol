//@compile-flags: -Zcodegen --emit=mir

type Wad is uint256;

using {addWad as +} for Wad global;

function addWad(Wad a, Wad b) pure returns (Wad) {
    return Wad.wrap(Wad.unwrap(a) + Wad.unwrap(b));
}

contract UdvtOperatorUnsupported {
    function add(Wad a, Wad b) external pure returns (Wad) {
        return a + b;
        //~^ ERROR: user-defined operators are not supported in codegen yet
        //~| HELP: unwrap the user-defined value type before using this operator
    }
}
