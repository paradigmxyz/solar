//@ codegen-matrix: standard
//@[mir] filecheck: --check-prefix=MIR
//@ run-call: wide => 25
//@ run-call: nested => 7
//@ run-call: small => 3

// Reading a struct element that was never assigned builds its default object.
// Structs with nested objects or at least four fields share one constructor
// across reads; a struct of a few value fields stays inline.
contract DefaultStructHelper {
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

    // MIR-LABEL: fn @wide(
    // MIR-COUNT-2: icall @[[WIDE:default_struct_[0-9]+]]
    function wide() external pure returns (uint256) {
        Wide[] memory values = new Wide[](2);
        values[0].d = 5;
        values[1].a = 20;
        return values[0].d + values[1].a + values[0].a;
    }

    // MIR-LABEL: fn @nested(
    // MIR: icall @[[NESTED:default_struct_[0-9]+]]
    function nested() external pure returns (uint256) {
        Nested[] memory values = new Nested[](1);
        values[0].inner.b = 7;
        return values[0].inner.b + values[0].inner.a;
    }

    // MIR-LABEL: fn @small(
    // MIR-NOT: icall @default_struct
    // MIR: alloc memorystruct<2>
    function small() external pure returns (uint256) {
        Small[] memory values = new Small[](1);
        values[0].b = 3;
        return values[0].a + values[0].b;
    }

    // MIR: fn @[[WIDE]]()
    // MIR: fn @[[NESTED]]()
}
