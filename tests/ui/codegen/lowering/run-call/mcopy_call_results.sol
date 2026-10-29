//@ compile-flags: --evm-version paris
//@ codegen-matrix: standard shared
//@[shared] compile-flags: -Osize -Zdump=mir
//@[shared] filecheck: --check-prefix=SHARED
//@ run-call: message(uint256,uint256) 0, 0 => 0x7c307c307c
//@ run-call: message(uint256,uint256) 3, 12 => 0x6161617c337c31327c616161616161616161616161
//@ run-call: message(uint256,uint256) 31, 32 => 0x616161616161616161616161616161616161616161616161616161616161617c33317c33327c6161616161616161616161616161616161616161616161616161616161616161
//@ run-call: message(uint256,uint256) 33, 100 => 0x6161616161616161616161616161616161616161616161616161616161616161617c33337c3130307c61616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161
//@ run-call: overlapping(uint256) 70 => 70, 0x2122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40
//@ run-call: overlapping(uint256) 33 => 33, 0x2100000000000000000000000000000000000000000000000000000000000000

// The pieces of a concatenation are call results of unknown length, whose objects the compiler
// does not allocate itself. Pre-Cancun targets copy each piece with a word loop, which the size
// objective shares through one helper; every piece, empty or ending inside a word, must land
// exactly. Once assembly moves the free memory pointer back to the first piece's payload, the
// output overlaps it and the shared helper must copy backward; the output's length word replaces
// the first word of the piece, so the test reads the second.
contract MCopyCallResults {
    // SHARED-LABEL: fn @message(
    // SHARED: icall @mcopy_words, {{v[0-9]+}}, {{v[0-9]+}}, {{v[0-9]+}}
    function message(uint256 a, uint256 b) external pure returns (bytes memory) {
        return bytes.concat(padding(a), "|", digits(a), "|", digits(b), "|", padding(b));
    }

    function overlapping(uint256 n) external pure returns (uint256 length, bytes32 second) {
        bytes memory value = counting(n);
        assembly {
            mstore(0x40, add(value, 32))
        }
        bytes memory joined = bytes.concat(value, "");
        length = joined.length;
        assembly {
            second := mload(add(joined, 64))
        }
    }

    function padding(uint256 n) internal pure returns (bytes memory result) {
        result = new bytes(n);
        for (uint256 i; i < n; ++i) {
            result[i] = "a";
        }
    }

    function counting(uint256 n) internal pure returns (bytes memory result) {
        result = new bytes(n);
        for (uint256 i; i < n; ++i) {
            result[i] = bytes1(uint8(i + 1));
        }
    }

    function digits(uint256 value) internal pure returns (bytes memory result) {
        uint256 length = 1;
        for (uint256 rest = value; rest >= 10; rest /= 10) {
            ++length;
        }
        result = new bytes(length);
        for (uint256 i = length; i > 0; --i) {
            result[i - 1] = bytes1(uint8(48 + value % 10));
            value /= 10;
        }
    }
}
