//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=abi,evm-ir,evm-ir-runtime --pretty-json

contract C {
    uint public x;

    constructor(uint value) {
        x = value;
    }
}
