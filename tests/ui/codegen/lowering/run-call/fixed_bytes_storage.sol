//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: read() => 170, 17, 48076, -2, 14544639
//@[none, gas, size] run-call: raw() => 0x000000000000000000000000000000000000000000000000ddeefffebbcc11aa
//@[none, gas, size] run-call: selector() => 0xea3d508a, 3929886858

contract FixedBytesStorage {
    bytes1 first;
    uint8 small;
    bytes2 second = 0xbbcc;
    int8 signed;
    bytes3 third;

    constructor() {
        first = 0xaa;
        small = 0x11;
        signed = -2;
        third = 0xddeeff;
    }

    function read() external view returns (uint8, uint8, uint16, int8, uint24) {
        return (uint8(first), small, uint16(second), signed, uint24(third));
    }

    function raw() external view returns (bytes32 value) {
        assembly {
            value := sload(0)
        }
    }

    function selector() external pure returns (bytes4 sig, uint32 numeric) {
        return (msg.sig, uint32(msg.sig));
    }
}
