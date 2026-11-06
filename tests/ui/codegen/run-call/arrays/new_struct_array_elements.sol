//@ codegen-matrix: standard
//@ run-call: wide => 25
//@ run-call: nested => 7
//@ run-call: small => 3

// `new S[](n)` stores a default object in every element, so reading an element
// that was never assigned follows its pointer without a zero test.
contract NewStructArrayElements {
    struct Small {
        uint256 a;
        uint256 b;
    }

    struct Wide {
        uint256 a;
        uint256 b;
        uint256 c;
        uint256 d;
    }

    struct Nested {
        Small inner;
        uint256 x;
    }

    function wide() external pure returns (uint256) {
        Wide[] memory values = new Wide[](2);
        values[0].d = 5;
        values[1].a = 20;
        return values[0].d + values[1].a + values[0].a;
    }

    function nested() external pure returns (uint256) {
        Nested[] memory values = new Nested[](1);
        values[0].inner.b = 7;
        return values[0].inner.b + values[0].inner.a;
    }

    function small() external pure returns (uint256) {
        Small[] memory values = new Small[](1);
        values[0].b = 3;
        return values[0].a + values[0].b;
    }
}
