//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir-final
//@[size] compile-flags: -Zdump=mir-final
//@[mir] filecheck: --check-prefix=NONE
//@[gas] filecheck:
//@[size] filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Each struct holds two copies of the one before it, so `S30` has 2^30 paths to `S0`. The decode
// shape shares one node per struct, so building, printing, and lowering it take time linear in
// the number of structs, and the text names each repeated struct once.
// NONE: abi_decode [tuple<$0 = tuple<$1 = {{.*}}<$29 = tuple<array<_, uint256>>, $29>, $28>{{.*}}, $0>], arg0

// The decoder of each struct calls the decoder of the struct inside it twice.
// CHECK-LABEL: fn @shared()
// CHECK: icall @decode_memory_type.[[S29:[0-9]+]],
// CHECK: icall @decode_memory_type.[[S29]],
// CHECK: fn @decode_memory_type.[[S29]](
// CHECK: icall @decode_memory_type.[[S28:[0-9]+]],
// CHECK: icall @decode_memory_type.[[S28]],

contract C {
    struct S0 { uint256[] a; }
    struct S1 { S0 a; S0 b; }
    struct S2 { S1 a; S1 b; }
    struct S3 { S2 a; S2 b; }
    struct S4 { S3 a; S3 b; }
    struct S5 { S4 a; S4 b; }
    struct S6 { S5 a; S5 b; }
    struct S7 { S6 a; S6 b; }
    struct S8 { S7 a; S7 b; }
    struct S9 { S8 a; S8 b; }
    struct S10 { S9 a; S9 b; }
    struct S11 { S10 a; S10 b; }
    struct S12 { S11 a; S11 b; }
    struct S13 { S12 a; S12 b; }
    struct S14 { S13 a; S13 b; }
    struct S15 { S14 a; S14 b; }
    struct S16 { S15 a; S15 b; }
    struct S17 { S16 a; S16 b; }
    struct S18 { S17 a; S17 b; }
    struct S19 { S18 a; S18 b; }
    struct S20 { S19 a; S19 b; }
    struct S21 { S20 a; S20 b; }
    struct S22 { S21 a; S21 b; }
    struct S23 { S22 a; S22 b; }
    struct S24 { S23 a; S23 b; }
    struct S25 { S24 a; S24 b; }
    struct S26 { S25 a; S25 b; }
    struct S27 { S26 a; S26 b; }
    struct S28 { S27 a; S27 b; }
    struct S29 { S28 a; S28 b; }
    struct S30 { S29 a; S29 b; }
    function shared(bytes memory data) public pure {
        abi.decode(data, (S30));
    }
}
