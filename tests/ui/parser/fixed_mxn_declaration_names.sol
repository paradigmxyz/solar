// Like solc, valid `[u]fixedMxN` type names are keywords and cannot name declarations.

contract fixed8x1 {} //~ ERROR: expected identifier, found keyword `fixed8x1`

type ufixed8x8 is uint8; //~ ERROR: expected identifier, found keyword `ufixed8x8`

struct fixed16x2 { //~ ERROR: expected identifier, found keyword `fixed16x2`
    uint a;
}

struct S {
    uint fixed8x2; //~ ERROR: expected identifier, found keyword `fixed8x2`
}

enum E {
    fixed8x1 //~ ERROR: expected identifier, found keyword `fixed8x1`
}

function ufixed256x80() {} //~ ERROR: expected identifier, found keyword `ufixed256x80`

contract C {
    uint x;
    uint fixed8x8; //~ ERROR: expected identifier, found keyword `fixed8x8`

    event fixed8x1(); //~ ERROR: expected identifier, found keyword `fixed8x1`
    error ufixed8x1(); //~ ERROR: expected identifier, found keyword `ufixed8x1`

    modifier fixed16x8() { //~ ERROR: expected identifier, found keyword `fixed16x8`
        _;
    }

    // The declaration fails, so `fixed128x18()` below is a type conversion.
    function fixed128x18() internal { //~ ERROR: expected identifier, found keyword `fixed128x18`
        x = 1;
    }

    function f(uint ufixed8x18) public {} //~ ERROR: expected identifier, found keyword `ufixed8x18`

    function g() public {
        uint fixed8x8; //~ ERROR: expected identifier, found keyword `fixed8x8`
    }

    function h() public returns (uint) {
        fixed128x18(); //~ ERROR: expected exactly one unnamed argument
        return x;
    }

    function y() public pure returns (uint r) {
        // Yul does not reserve these names.
        assembly {
            let fixed8x8 := 1
            r := fixed8x8
        }
    }
}
