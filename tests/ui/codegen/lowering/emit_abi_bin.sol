//@ check-pass
//@ revisions: evmir bin
//@ignore-host: windows
//@[evmir] compile-flags: -Zcodegen --emit=abi -Zdump=evm-ir,evm-ir-runtime --pretty-json
//@[bin] compile-flags: -Zcodegen --emit=abi,bin,bin-runtime --pretty-json

contract C {
    uint public x;

    constructor(uint value) {
        x = value;
    }
}
