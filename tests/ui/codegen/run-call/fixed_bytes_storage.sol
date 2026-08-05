//@ run-call: read() => 170, 17, 48076, -2, 14544639
//@ run-call: raw() => 0x000000000000000000000000000000000000000000000000ddeefffebbcc11aa
//@ run-call: selector() => 0xea3d508a, 3929886858

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
