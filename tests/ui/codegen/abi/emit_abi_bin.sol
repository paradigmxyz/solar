//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=abi,bin,bin-runtime --pretty-json

contract C {
    uint public x;

    constructor(uint value) {
        x = value;
    }
}
