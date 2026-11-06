//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: C::t_bytes => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: C::t_string => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: C::t_words => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: C::t_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041

// A storage `bytes`, `string` or dynamic array whose stored length is `2^64`, which only assembly
// can write, reverts with `Panic(0x41)` when copied to memory. solc via-IR encodes the copy into
// unbounded memory and runs out of gas. See CODEGEN-015 in `docs/SOLC_DIVERGENCE.md`.

contract C {
    bytes b;
    string s;
    uint256[] words;
    uint8[] packed;

    function t_bytes() external returns (uint256) {
        assembly { sstore(b.slot, add(shl(65, 1), 1)) }
        bytes memory m = b;
        return m.length;
    }

    function t_string() external returns (uint256) {
        assembly { sstore(s.slot, add(shl(65, 1), 1)) }
        string memory m = s;
        return bytes(m).length;
    }

    function t_words() external returns (uint256) {
        assembly { sstore(words.slot, shl(64, 1)) }
        uint256[] memory m = words;
        return m.length;
    }

    function t_packed() external returns (uint256) {
        assembly { sstore(packed.slot, shl(64, 1)) }
        uint8[] memory m = packed;
        return m.length;
    }
}
