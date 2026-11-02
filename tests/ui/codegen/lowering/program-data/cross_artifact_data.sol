//@ revisions: constructor runtime mir
//@[constructor] compile-flags: -Ogas -Zdump=evm-ir
//@[constructor] filecheck: --check-prefix=CONSTRUCTOR
//@[runtime] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck: --check-prefix=MIR

// MIR-NOT: literal_0
// CONSTRUCTOR-LABEL: @module C_deployment
// CONSTRUCTOR: literal_{{[0-9]+}}: hex"aaaaaaaa
// CONSTRUCTOR-NOT: hex"11111111
// RUNTIME-LABEL: @module C_runtime
// RUNTIME: literal_{{[0-9]+}}: hex"11111111
// RUNTIME-NOT: hex"aaaaaaaa
contract C {
    event ConstructorData(bytes data);

    constructor() {
        emit ConstructorData(hex"aaaaaaaaa54dca182530bb1d6d132cded6237b2ed91e3f721fcb1971174494d6493c9d5c3460be31201e69fedaa0eee8b9997f5c7c2999fdafe593253cd654af4dfad71427a0aeb3fee9232f8af2211f9ee491c5b10becb5563bfc1e6f93427ecbc8fe2955e5cd8e46dc8ed4b7c2764d2a5a4d767706f85d8690024ad6bda3401be9c8cbccc935f6cd1f61226ae15338ae1a34004d33ba0d246ac04c81b1baf23e3bf9eef5f79f2b4934af87f5520b69b94b0d982e85bb55b672a872637acd74");
    }

    function runtimeData() external pure returns (bytes memory) {
        return hex"1111111166fcb60e0e8ff18463b0e4b2ba29703474f064ac68f700f5b02b3dc666f45bdeaa2ccaedcd2b5157410e4dee4af2b34f430a073447de636c0e806c957ba684d6431fb5ead7424d09e15d024c5848f23d1fa6f7361d7f618d1532e70e20e2a6668de7f47e8467e546d53ec8e2a1257bdb256c9b3e4fbb498146ef7030cbf9537252dcceadd764b6a32fbb09adeae109c4a997203975352b878b145c8a42d884cf4cfda72d8e1d5dd92589082d852a7122873ee805add58942167a3852";
    }
}
