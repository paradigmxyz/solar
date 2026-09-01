//@ codegen-matrix: standard
//@ run-call: encodeStruct => 0xeda0614208a532abac4ee92dfe460080932ac899a8f7d3a36ef50b3b9cccbe23
//@ run-call: encodeArray => 0x97111a84a686589f972d5c1ad374a79bbcd945e7f919d275b99263f71ec6b3b9
//@ run-call-fail: encodeBadEnum => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021

// Memory aggregates with narrow fields are encoded in place: each word is
// cleaned while encoding, like solc, instead of copying the whole object into
// a canonical layout first. Fields dirtied through inline assembly still
// encode as their declared type, and an out-of-range enum still panics.
contract AbiEncodeDirtyMemoryFields {
    enum Mode {
        Off,
        On
    }

    struct S {
        uint8 small;
        bool flag;
        Mode mode;
        address who;
        int8 signed;
        bytes2 pair;
    }

    function dirty() internal pure returns (S memory s) {
        s.small = 1;
        s.flag = true;
        s.mode = Mode.On;
        s.who = address(1);
        s.signed = -1;
        s.pair = 0x1234;
        assembly {
            mstore(s, 0x1ff)
            mstore(add(s, 0x20), 0x102)
            mstore(add(s, 0x60), 0x1000000000000000000000000000000000000000000000000000000000000002)
            mstore(add(s, 0x80), 0xfe)
            mstore(add(s, 0xa0), 0x1234567800000000000000000000000000000000000000000000000000000000)
        }
    }

    function encodeStruct() external pure returns (bytes32) {
        return keccak256(abi.encode(dirty()));
    }

    function encodeArray() external pure returns (bytes32) {
        uint8[] memory values = new uint8[](2);
        values[0] = 3;
        values[1] = 4;
        assembly {
            mstore(add(values, 0x20), 0xff03)
            mstore(add(values, 0x40), 0x104)
        }
        return keccak256(abi.encode(values));
    }

    function encodeBadEnum() external pure returns (bytes memory) {
        S memory s = dirty();
        assembly {
            mstore(add(s, 0x40), 5)
        }
        return abi.encode(s);
    }
}
