//@ check-pass
//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract StorageLongBytesReturn {
    string public s;
    bytes public b;

    constructor() {
        assembly {
            sstore(s.slot, 0x41)
            mstore(0x00, s.slot)
            let sData := keccak256(0x00, 0x20)
            sstore(sData, 0x6162636465666768696a6b6c6d6e6f707172737475767778797a414243444546)

            sstore(b.slot, 0x43)
            mstore(0x00, b.slot)
            let bData := keccak256(0x00, 0x20)
            sstore(bData, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)
            sstore(add(bData, 1), 0x2100000000000000000000000000000000000000000000000000000000000000)
        }
    }

    function getS() public view returns (string memory) {
        return s;
    }

    function getB() public view returns (bytes memory) {
        return b;
    }
}
