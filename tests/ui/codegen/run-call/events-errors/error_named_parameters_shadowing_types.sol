//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call-fail: f => 0x33a54193000000000000000000000000000000000000000000000000000000000000002a
//@ run-call-fail: g => 0x374b93870000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002a
// ported-from: test/libsolidity/semanticTests/errors/named_parameters_shadowing_types.sol

pragma abicoder v2;

contract ErrorNamedParametersShadowingTypes {
    enum EnumType {A, B, C}

    struct StructType {
        uint x;
    }

    error E1(StructType StructType);
    error E2(EnumType StructType, StructType EnumType);

    function f() public pure {
        revert E1({StructType: StructType(42)});
    }

    function g() public pure {
        revert E2({EnumType: StructType(42), StructType: EnumType.B});
    }
}
