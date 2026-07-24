//@ revisions: evmir bin
//@ignore-host: windows
//@[evmir] compile-flags: -Zcodegen --emit=abi -Zdump=evm-ir,evm-ir-runtime --pretty-json
//@[evmir] filecheck: --check-prefix=EVMIR
//@[bin] compile-flags: -Zcodegen --emit=abi,bin,bin-runtime --pretty-json
//@[bin] filecheck: --check-prefix=BIN

// EVMIR: "type": "constructor"
// EVMIR: "name": "value"
// EVMIR: "name": "x"
// EVMIR-LABEL: @module deployment
// EVMIR: mload
// EVMIR: sstore
// EVMIR: return
// EVMIR-LABEL: @module runtime
// EVMIR: push 0xc55699c
// EVMIR: sload
// EVMIR: return
// BIN: "name": "value"
// BIN: "name": "x"
// BIN: "bin": "{{[0-9a-f]+}}"
// BIN: "bin-runtime": "{{[0-9a-f]+}}"
contract C {
    uint public x;

    constructor(uint value) {
        x = value;
    }
}
