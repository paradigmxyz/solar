//@compile-flags: -Ztypeck

type MyUint is uint256;
type MyAddress is address;
type MyInt is int128;
type Price is uint256;
type Amount is uint128;

contract UDVTTests {
    // Valid: wrap (underlying -> UDVT).
    function testWrap(uint256 x, address a, int128 i) public pure {
        MyUint u = MyUint(x);
        MyAddress ma = MyAddress(a);
        MyInt mi = MyInt(i);
    }

    // Valid: unwrap (UDVT -> underlying).
    function testUnwrap(MyUint u, MyAddress ma, MyInt mi) public pure {
        uint256 x = uint256(u);
        address a = address(ma);
        int128 i = int128(mi);
    }

    // Valid: round-trip conversion.
    function testRoundTrip(uint256 x) public pure returns (uint256) {
        MyUint u = MyUint(x);
        return uint256(u);
    }
}

contract UDVTErrors {
    // Invalid: cannot convert UDVT to incompatible underlying type.
    function testWrongUnderlying(Price p) public pure {
        int256 x = int256(p); //~ ERROR: invalid explicit type conversion
    }

    // Invalid: cannot convert one UDVT to another UDVT.
    function testUDVTtoUDVT(MyUint u) public pure {
        Price p = Price(u); //~ ERROR: invalid explicit type conversion
    }

    // Invalid: cannot convert UDVT to different size underlying.
    function testUDVTtoWrongSize(Price p) public pure {
        uint128 x = uint128(p); //~ ERROR: invalid explicit type conversion
    }
}
