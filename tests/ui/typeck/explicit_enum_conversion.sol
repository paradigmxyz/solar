//@compile-flags: -Ztypeck
contract C {
    enum TrafficLight {
        Red,
        Yellow,
        Green
    }

    function validEnumToInteger(TrafficLight t) public pure {
        uint8 u8 = uint8(t);
        uint16 u16 = uint16(t);
        uint32 u32 = uint32(t);
        uint64 u64 = uint64(t);
        uint128 u128 = uint128(t);
        uint256 u256 = uint256(t);
    }

    function validIntegerToEnum(uint8 u8, int256 i256) public pure {
        TrafficLight t = TrafficLight(u8);
        TrafficLight t2 = TrafficLight(i256);
        TrafficLight t3 = TrafficLight(1);
    }

    function invalidEnumToBytes(TrafficLight t) public pure {
        bytes1 b1 = bytes1(t); //~ ERROR: cannot convert `enum C.TrafficLight` to `bytes1`
        bytes32 b32 = bytes32(t); //~ ERROR: cannot convert `enum C.TrafficLight` to `bytes32`
    }

    function invalidEnumToAddress(TrafficLight t) public pure {
        address addr = address(t); //~ ERROR: cannot convert `enum C.TrafficLight` to `address`
    }

    function invalidEnumToBool(TrafficLight t) public pure {
        bool b = bool(t); //~ ERROR: cannot convert `enum C.TrafficLight` to `bool`
    }
}

