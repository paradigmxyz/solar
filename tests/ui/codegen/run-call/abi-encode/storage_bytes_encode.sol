//@ filecheck:
//@ codegen-matrix: standard
//@ run-call: encodings => true
//@ run-call: referencesAndConversions => true
//@ run-call: externalCall => true
//@ run-call: slotNinetySix => true
//@ run-call: packedGrowth => 0
//@ run-call: encodeGrowth => 0
//@ run-call: concatGrowth => 0
//@ run-call: eventGrowth => 0
//@ run-call: packedAfterMutation => true
//@ run-call: encodeAfterMutation => true
//@ run-call: concatAfterMutation => true
//@ run-call: callAfterMutation => true
//@ run-call-fail: failLong() => 0x4eb948720000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003968656c6c6f2073746f7261676520737472696e672074686174206973206c6f6e676572207468616e207468697274792d6f6e6520627974657300000000000000
//@ run-call-fail: failAfterMutation() => 0x5ef4980a0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000017800000000000000000000000000000000000000000000000000000000000000
// Storage `bytes` and `string` operands of ABI encodings are read from storage straight into
// the output, after the operands that follow them ran, with no memory copy and no free memory
// pointer growth.
contract Test {
    event Ev(string data);
    error Err(string data);
    error Err2(string data, uint256 value);

    string[] internal strs;
    string internal long = "hello storage string that is longer than thirty-one bytes";
    bytes internal big;
    mapping(uint256 => bytes) internal map;

    struct Holder {
        uint256 id;
        string name;
    }

    Holder internal holder;
    uint256[90] internal gap;
    // Slot 96 is also the address of the zero slot in memory.
    string internal atSlot96 = "hello storage string that is longer than thirty-one bytes";

    constructor() {
        strs.push("");
        strs.push("short");
        strs.push("0123456789012345678901234567890");
        strs.push("01234567890123456789012345678901");
        strs.push("012345678901234567890123456789012");
        strs.push("0123456789012345678901234567890123456789012345678901234567890123");
        strs.push("0123456789012345678901234567890123456789012345678901234567890123456789");
        big = new bytes(1000);
        map[3] = "mapped storage bytes that are longer than thirty-one bytes";
        holder = Holder(1, "holder name that is longer than thirty-one bytes");
    }

    function fp() private pure returns (uint256 pointer) {
        assembly ("memory-safe") {
            pointer := mload(0x40)
        }
    }

    function hashArg(string calldata value) external pure returns (bytes32) {
        return keccak256(bytes(value));
    }

    function hashArgs(string calldata value, uint256 extra) external pure returns (bytes32) {
        return keccak256(abi.encode(value, extra));
    }

    function encodings() external view returns (bool) {
        for (uint256 i; i < strs.length; ++i) {
            string memory m = strs[i];
            if (
                keccak256(abi.encodePacked(strs[i], uint8(7), strs[i]))
                    != keccak256(abi.encodePacked(m, uint8(7), m))
            ) return false;
            if (
                keccak256(abi.encodePacked(uint8(1), uint16(2), strs[i]))
                    != keccak256(abi.encodePacked(uint8(1), uint16(2), m))
            ) return false;
            if (
                keccak256(abi.encodePacked(strs[i])) != keccak256(abi.encodePacked(m))
            ) return false;
            if (
                keccak256(abi.encodePacked(uint8(7), strs[i], uint16(9), strs[i], "tail"))
                    != keccak256(abi.encodePacked(uint8(7), m, uint16(9), m, "tail"))
            ) return false;
            if (
                keccak256(abi.encode(uint256(1), strs[i], uint8(2), strs[i]))
                    != keccak256(abi.encode(uint256(1), m, uint8(2), m))
            ) return false;
            if (
                keccak256(abi.encodeWithSelector(bytes4(0x12345678), strs[i], uint8(3)))
                    != keccak256(abi.encodeWithSelector(bytes4(0x12345678), m, uint8(3)))
            ) return false;
            if (
                keccak256(abi.encodeCall(this.hashArgs, (strs[i], 5)))
                    != keccak256(abi.encodeCall(this.hashArgs, (m, 5)))
            ) return false;
            bytes memory packed = abi.encodePacked(uint8(7), strs[i], uint16(9), strs[i]);
            bytes memory expected = abi.encodePacked(uint8(7), m, uint16(9), m);
            if (keccak256(packed) != keccak256(expected) || packed.length != expected.length) {
                return false;
            }
            bytes31 prefix = 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f;
            if (keccak256(abi.encodePacked(prefix, strs[i])) != keccak256(abi.encodePacked(prefix, m))) {
                return false;
            }
            bytes memory unaligned = abi.encodePacked(prefix, strs[i]);
            if (keccak256(unaligned) != keccak256(abi.encodePacked(prefix, m))) return false;
            string memory joined = string.concat(strs[i], "-", strs[i]);
            if (keccak256(bytes(joined)) != keccak256(bytes(string.concat(m, "-", m)))) {
                return false;
            }
            bytes memory joinedBytes = bytes.concat(bytes(strs[i]), hex"01", bytes(strs[i]));
            if (keccak256(joinedBytes) != keccak256(bytes.concat(bytes(m), hex"01", bytes(m)))) {
                return false;
            }
        }
        return true;
    }

    function referencesAndConversions() external view returns (bool) {
        string storage pointer = holder.name;
        string memory m = pointer;
        bytes memory viaMapping = map[3];
        if (
            keccak256(abi.encodePacked(pointer, holder.name, map[3], bytes(pointer)))
                != keccak256(abi.encodePacked(m, m, viaMapping, bytes(m)))
        ) return false;
        if (
            keccak256(abi.encode(pointer, holder.id, map[3]))
                != keccak256(abi.encode(m, holder.id, viaMapping))
        ) return false;
        string memory joined = string.concat(pointer, string(map[3]));
        return keccak256(bytes(joined)) == keccak256(bytes(string.concat(m, string(viaMapping))));
    }

    function externalCall() external view returns (bool) {
        for (uint256 i; i < strs.length; ++i) {
            if (this.hashArg(strs[i]) != keccak256(bytes(strs[i]))) return false;
            if (this.hashArgs(strs[i], 9) != keccak256(abi.encode(strs[i], 9))) return false;
        }
        return this.hashArg(string(big)) == keccak256(big);
    }

    // CHECK-LABEL: fn @packedGrowth
    // CHECK: keccak256_packed (storage_bytes {{v[0-9]+|[0-9]+}}, uint256 1)
    function slotNinetySix() external returns (bool) {
        string memory m = atSlot96;
        if (keccak256(abi.encode(atSlot96, uint8(1))) != keccak256(abi.encode(m, uint8(1)))) {
            return false;
        }
        if (keccak256(abi.encodeCall(this.hashArgs, (atSlot96, 5))) != keccak256(abi.encodeCall(this.hashArgs, (m, 5)))) {
            return false;
        }
        if (keccak256(abi.encodePacked(atSlot96, uint8(1))) != keccak256(abi.encodePacked(m, uint8(1)))) {
            return false;
        }
        if (this.hashArgs(atSlot96, 9) != keccak256(abi.encode(m, 9))) return false;
        (bool ok, bytes memory data) = address(this).call(abi.encodeCall(this.failSlot96, ()));
        return !ok && keccak256(data) == keccak256(abi.encodeWithSelector(Err.selector, m));
    }

    function failSlot96() external view {
        revert Err(atSlot96);
    }

    function packedGrowth() external view returns (uint256) {
        uint256 start = fp();
        bytes32 hash = keccak256(abi.encodePacked(big, uint256(1)));
        uint256 end = fp();
        require(hash != bytes32(0));
        return end - start;
    }

    // CHECK-LABEL: fn @encodeGrowth
    // CHECK: abi_encode [storage_bytes], scratch
    function encodeGrowth() external view returns (uint256) {
        uint256 start = fp();
        bytes32 hash = keccak256(abi.encode(big));
        uint256 end = fp();
        require(hash != bytes32(0));
        return end - start;
    }

    // CHECK-LABEL: fn @concatGrowth
    // CHECK: abi_encode_packed (storage_bytes {{v[0-9]+|[0-9]+}}, data hex"78")
    function concatGrowth() external view returns (uint256) {
        uint256 start = fp();
        bytes memory joined = bytes.concat(big, "x");
        uint256 end = fp();
        return end - start - 32 - (joined.length + 31) / 32 * 32;
    }

    // CHECK-LABEL: fn @eventGrowth
    // CHECK: abi_encode [storage_bytes], scratch
    function eventGrowth() external returns (uint256) {
        uint256 start = fp();
        emit Ev(long);
        return fp() - start;
    }

    function mutate() internal returns (uint256) {
        long = "x";
        return 1;
    }

    function packedAfterMutation() external returns (bool) {
        bytes memory packed = abi.encodePacked(long, mutate());
        return keccak256(packed) == keccak256(abi.encodePacked("x", uint256(1)));
    }

    function encodeAfterMutation() external returns (bool) {
        bytes memory encoded = abi.encode(long, mutate());
        return keccak256(encoded) == keccak256(abi.encode("x", uint256(1)));
    }

    function concatAfterMutation() external returns (bool) {
        string memory joined = string.concat(long, string(abi.encodePacked(mutate())));
        return keccak256(bytes(joined)) == keccak256(bytes.concat("x", abi.encodePacked(uint256(1))));
    }

    function callAfterMutation() external returns (bool) {
        return this.hashArgs(long, mutate()) == keccak256(abi.encode("x", uint256(1)));
    }

    // CHECK-LABEL: fn @failLong
    // CHECK: require<custom_error [storage_bytes]>
    function failLong() external view {
        revert Err(long);
    }

    function failAfterMutation() external {
        revert Err2(long, mutate());
    }
}
