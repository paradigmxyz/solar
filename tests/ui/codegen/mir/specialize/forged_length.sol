//@ codegen-matrix: standard
//@ run-call: forged => 7
//@ run-call: plain 0x616263 => 3
//@ run-call: narrow => 18446744073709551615
// Specialization prices a helper in a module of its own, which holds no assembly. Another function
// of the contract forges a length with assembly, so the helper's length test and cleanup remain.
contract ForgedLength {
    function helper(bytes memory data, uint256 mode) internal pure returns (uint256) {
        if (mode == 1) {
            if (data.length > 2 ** 200) return 7;
            return 1;
        }
        uint256 acc;
        for (uint256 i; i < data.length; i++) {
            acc += uint8(data[i]) * mode;
        }
        return acc;
    }

    function width(bytes memory data, uint256 mode) internal pure returns (uint256) {
        if (mode == 1) {
            return uint64(data.length);
        }
        uint256 acc;
        for (uint256 i; i < data.length; i++) {
            acc += uint8(data[i]) * mode;
        }
        return acc;
    }

    function forged() public pure returns (uint256) {
        bytes memory data = new bytes(0);
        assembly {
            mstore(data, not(0))
        }
        return helper(data, 1);
    }

    function narrow() public pure returns (uint256) {
        bytes memory data = new bytes(0);
        assembly {
            mstore(data, not(0))
        }
        return width(data, 1);
    }

    function plain(bytes memory data) public pure returns (uint256) {
        return helper(data, 1) * width(data, 1);
    }

    function other(bytes memory data) public pure returns (uint256) {
        return helper(data, 1) + helper(data, 1) + width(data, 1) + width(data, 1);
    }
}
