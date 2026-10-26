//@ codegen-matrix: standard abi
//@[abi] compile-flags: -O none -Zdump=mir --emit=abi --pretty-json
contract C {
    uint public x;

    constructor(uint value) {
        x = value;
    }
}
