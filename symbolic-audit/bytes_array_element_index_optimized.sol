// Indexing bytes inside a `bytes[2] memory` element from a helper function.
// Under the default optimizer solar reverts with Panic(0x32); under -Onone it
// agrees with solc, which returns 1. No calldata is involved. Regression on
// 1738b4454: the solc test calldata_2d_bytes_to_memory_2.sol agreed on
// earlier builds.
contract BytesArrayElementIndexOptimized {
    function build() internal pure returns (bytes[2] memory m) {
        m[0] = hex"6162";
        m[1] = hex"6162";
    }

    // solc 1, solar Panic(0x32).
    function twoAsserts() external pure returns (uint256) {
        return checkBoth(build());
    }

    function checkBoth(bytes[2] memory m) internal pure returns (uint256) {
        assert(m[0][0] == m[1][0]);
        assert(m[0][1] == m[1][1]);
        return 1;
    }

    // solc 0xc3, solar Panic(0x32).
    function sumSecond() external pure returns (uint256) {
        return sumElement1(build());
    }

    function sumElement1(bytes[2] memory m) internal pure returns (uint256) {
        return uint8(m[1][0]) + uint8(m[1][1]);
    }

    // Agrees with solc: same reads on element 0.
    function sumFirst() external pure returns (uint256) {
        return sumElement0(build());
    }

    function sumElement0(bytes[2] memory m) internal pure returns (uint256) {
        return uint8(m[0][0]) + uint8(m[0][1]);
    }

    // Agrees with solc: one comparison only.
    function oneAssert() external pure returns (uint256) {
        return checkOne(build());
    }

    function checkOne(bytes[2] memory m) internal pure returns (uint256) {
        assert(m[0][1] == m[1][1]);
        return 1;
    }
}
