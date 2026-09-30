//@ codegen-matrix: standard
//@ run-call: claims => true, true, true, true, false, false, 0x8000000000000000000000000000000000000000000000000000000000000003, 0x0000000000000000000000000000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: unclaims => false, true, false, 0x0000000000000000000000000000000000000000000000000000000000000002
//@ run-call: toggles => true, false, true
//@ run-call: getters => true, false
//@ run-call: extremes => true, true, false, 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: small => true, false, true, 0x0000000000000100000000000000000000000000000000000000000000000080
//@ run-call: plainStaysStandard => 0x0000000000000000000000000000000000000000000000000000000000000001

// A mapping from an unsigned integer to `bool` documented `@custom:solar-bitmap`
// keeps the value for key `k` in bit `k % 256` of the word at
// keccak256((k / 256) . slot). Reads and writes behave as without the tag; raw
// reads of the words show the layout.
contract Bitmaps {
    /// @custom:solar-bitmap
    mapping(uint256 => bool) public claimed;
    /// @custom:solar-bitmap
    mapping(uint8 => bool) flags;
    mapping(uint256 => bool) plain;

    function word(uint256 index, uint256 slot) internal view returns (bytes32 value) {
        bytes32 location = keccak256(abi.encode(index, slot));
        assembly {
            value := sload(location)
        }
    }

    function claims()
        external
        returns (bool, bool, bool, bool, bool, bool, bytes32, bytes32, bytes32)
    {
        claimed[0] = true;
        claimed[1] = true;
        claimed[255] = true;
        claimed[256] = true;
        return (
            claimed[0],
            claimed[1],
            claimed[255],
            claimed[256],
            claimed[2],
            claimed[257],
            word(0, 0),
            word(1, 0),
            word(2, 0)
        );
    }

    function unclaims() external returns (bool, bool, bool, bytes32) {
        claimed[0] = true;
        claimed[1] = true;
        claimed[2] = true;
        claimed[0] = false;
        delete claimed[2];
        return (claimed[0], claimed[1], claimed[2], word(0, 0));
    }

    function toggles() external returns (bool, bool, bool) {
        claimed[9] = !claimed[9];
        bool first = claimed[9];
        claimed[9] = !claimed[9];
        bool second = claimed[9];
        claimed[9] = !claimed[9];
        return (first, second, claimed[9]);
    }

    function getters() external returns (bool, bool) {
        claimed[77] = true;
        return (this.claimed(77), this.claimed(78));
    }

    // The highest keys share the last word, and the top bit holds the last key.
    function extremes() external returns (bool, bool, bool, bytes32) {
        uint256 last = type(uint256).max;
        claimed[last] = true;
        claimed[last - 1] = true;
        claimed[last - 1] = false;
        claimed[last - 256] = true;
        return (claimed[last], claimed[last - 256], claimed[last - 1], word(last >> 8, 0));
    }

    // Every `uint8` key lives in the mapping's one word.
    function small() external returns (bool, bool, bool, bytes32) {
        flags[7] = true;
        flags[200] = true;
        return (flags[7], flags[8], flags[200], word(0, 1));
    }

    // An untagged mapping keeps the standard layout.
    function plainStaysStandard() external returns (bytes32 value) {
        plain[5] = true;
        bytes32 location = keccak256(abi.encode(uint256(5), uint256(2)));
        assembly {
            value := sload(location)
        }
    }
}
