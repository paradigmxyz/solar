//@ignore-host: windows
//@compile-flags: --emit=abi,bin,bin-runtime --pretty-json

contract C {
    uint public x;

    constructor(uint value) {
        x = value;
    }
}
