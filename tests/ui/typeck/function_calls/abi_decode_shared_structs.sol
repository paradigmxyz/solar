// The `abi.decode` size check visits each struct once, so a chain of structs that each hold two
// copies of the previous one takes 31 visits, not 2^30.
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
