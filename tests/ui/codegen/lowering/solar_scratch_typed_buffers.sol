//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: duplicate [] => false
//@ run-call: duplicate [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002] => false
//@ run-call: duplicate [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000001] => true
//@ run-call: copy 0x => 0x
//@ run-call: copy 0x010203 => 0x010203
//@ run-call: hashAuth 0x010203, 7 => 0x58b68a24ec6c77fbdf76de256871f30296cf80af03114c8e85a7ab208088fd82, true
//@ run-call: hashAfterCall 0x010203 => 0x58b68a24ec6c77fbdf76de256871f30296cf80af03114c8e85a7ab208088fd85

contract Test {
    struct Auth {
        bytes data;
        uint256 counter;
    }

    // Reinterpreting an array without changing its allocation preserves the object bounds.
    function words(address[] memory values) internal pure returns (uint256[] memory result) {
        /// @solidity memory-safe-assembly
        assembly { result := values }
    }

    function duplicate(address[] memory values) external pure returns (bool) {
        return hasDuplicate(words(values));
    }

    function hasDuplicate(uint256[] memory values) internal pure returns (bool found) {
        /// @custom:solar-scratch
        {
            uint256[] memory seen = new uint256[](8);
            for (uint256 i; i < values.length; ++i) {
                uint256 slot = values[i] % seen.length;
                if (seen[slot] != 0) found = true;
                seen[slot] = i + 1;
            }
        }
    }

    // A scratch input and fixed-array table can feed checked writes to an older output buffer.
    function copy(bytes memory input) external pure returns (bytes memory result) {
        result = new bytes(input.length);
        /// @custom:solar-scratch
        {
            bytes memory padded = bytes.concat(input, bytes32(0));
            uint256[4] memory table;
            uint256 slot = input.length % 4;
            seed(table, slot, input.length);
            copyBytes(result, padded, table[slot]);
        }
        truncate(result, input.length);
    }

    function seed(uint256[4] memory table, uint256 slot, uint256 count) internal pure {
        table[slot] = count;
    }

    function copyBytes(bytes memory output, bytes memory input, uint256 count) internal pure {
        for (uint256 i; i < count; ++i) output[i] = input[i];
    }

    function truncate(bytes memory buffer, uint256 length) internal pure {
        require(length <= buffer.length);
        assembly ("memory-safe") { mstore(buffer, length) }
    }

    // Nested hashes consume the bytes, while the decoded fields and digest remain scalar data.
    function hashAuth(bytes memory data, uint256 counter) external pure returns (bytes32 hash, bool ok) {
        return hashFields(Auth(data, counter));
    }

    function hashFields(Auth memory auth) internal pure returns (bytes32 hash, bool ok) {
        bytes memory data = auth.data;
        ok = auth.counter != 0;
        /// @custom:solar-scratch
        { hash = sha256(abi.encodePacked(data, sha256(data))); }
        if (ok) hash ^= bytes32(auth.counter);
    }

    // A raw call's input range is disjoint from the FMP slot when its caller supplies fresh bytes.
    function hashAfterCall(bytes memory data) external view returns (bytes32 hash) {
        bytes memory payload = abi.encodePacked(data);
        bytes32 first = hashWord(payload);
        /// @custom:solar-scratch
        { hash = sha256(abi.encodePacked(data, first)); }
    }

    function hashWord(bytes memory payload) internal view returns (bytes32 word) {
        assembly ("memory-safe") {
            if iszero(staticcall(gas(), 2, add(payload, 32), mload(payload), 0, 32)) { revert(0, 0) }
            word := mload(0)
        }
    }
}
