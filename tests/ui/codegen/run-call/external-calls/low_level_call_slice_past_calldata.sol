//@ codegen-matrix: standard
//@ run-call: echoPast 28 => 0x0000001c00000000000000000000000000000000000000000000000000000000
//@ run-call: echo 0x1234 => 0x1234

// As in solc, a low-level call copies calldata `bytes` input without checking
// that it lies within the calldata: a slice that inline assembly makes run
// past the end reads zeros there. The identity precompile echoes the input.
contract LowLevelCallSlicePastCalldata {
    function echoPast(uint256 extra) external view returns (bytes memory result) {
        bool success;
        (success, result) = address(4).staticcall(_slice(msg.data.length - 4, 4 + extra));
        require(success);
    }

    function echo(bytes calldata data) external view returns (bytes memory result) {
        bool success;
        (success, result) = address(4).staticcall(data);
        require(success);
    }

    function _slice(uint256 offset, uint256 length) private pure returns (bytes calldata s) {
        assembly {
            s.offset := offset
            s.length := length
        }
    }
}
