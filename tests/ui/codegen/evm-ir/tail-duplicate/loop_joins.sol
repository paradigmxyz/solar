//@ codegen-matrix: standard
//@ run-call: count 0x => 0
//@ run-call: count 0x61 => 1
//@ run-call: count 0xc2a2e282acf09f988061 => 4
//@ run-call: count 0x6161616161616161616161616161616161616161616161616161616161616161 => 32
//@ run-call: count 0x6161616161616161616161616161616161616161616161616161616161616161c2a2 => 33
//@ run-call: count 0xf800000000fc0000000000 => 2
//@ run-call: countThree 0x, 0x, 0x => 0
//@ run-call: countThree 0x616263, 0xe282ac, 0xf09f9880 => 5

contract LoopJoins {
    function count(bytes memory value) public pure returns (uint256 n) {
        unchecked {
            for (uint256 i; i < value.length; ++n) {
                bytes1 b = value[i];
                if (b < 0x80) {
                    i += 1;
                } else if (b < 0xe0) {
                    i += 2;
                } else if (b < 0xf0) {
                    i += 3;
                } else if (b < 0xf8) {
                    i += 4;
                } else if (b < 0xfc) {
                    i += 5;
                } else {
                    i += 6;
                }
            }
        }
    }

    // Multiple calls place the shared loop beyond the packed-address prefix.
    function countThree(bytes memory a, bytes memory b, bytes memory c) external pure returns (uint256) {
        return count(a) + count(b) + count(c);
    }
}
