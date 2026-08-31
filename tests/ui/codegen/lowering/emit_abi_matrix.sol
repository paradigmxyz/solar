//@ codegen-matrix: standard
//@[mir] compile-flags: --emit=abi --pretty-json
contract C {
    uint public x;

    constructor(uint value) {
        x = value;
    }
}
