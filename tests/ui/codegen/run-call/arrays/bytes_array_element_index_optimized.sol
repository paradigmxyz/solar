//@ codegen-matrix: standard
//@ run-call: sumSecond => 195
//@ run-call: twoAsserts => 1

contract BytesArrayElementIndexOptimized {
    function build() internal pure returns (bytes[2] memory m) {
        m[0] = hex"6162";
        m[1] = hex"6162";
    }

    function sumSecond() external pure returns (uint256) {
        return sumElement1(build());
    }

    function sumElement1(bytes[2] memory m) internal pure returns (uint256) {
        return uint8(m[1][0]) + uint8(m[1][1]);
    }

    function twoAsserts() external pure returns (uint256) {
        bytes[2] memory m = build();
        assert(m[0][0] == m[1][0]);
        assert(m[0][1] == m[1][1]);
        return 1;
    }
}
