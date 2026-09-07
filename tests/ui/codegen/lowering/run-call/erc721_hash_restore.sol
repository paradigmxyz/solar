//@ codegen-matrix: standard
//@ run-call: hashes 1, 2 => 0xaf0c224d3b3dfde7ec21b9b700f9eec830de5a6067303eebc57ba60855f20b36, 0xaf0c224d3b3dfde7ec21b9b700f9eec830de5a6067303eebc57ba60855f20b36
//@ run-call: hashes 115792089237316195423570985008687907853269984665640564039457584007913129639935, 0 => 0xa2bbe749ff320c635bda208d46500cdf8c24e5024bc92bd0e4a8f249cb489218, 0xaf0c224d3b3dfde7ec21b9b700f9eec830de5a6067303eebc57ba60855f20b36
//@ run-call: hashes 0, 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0xaf0c224d3b3dfde7ec21b9b700f9eec830de5a6067303eebc57ba60855f20b36, 0xa2bbe749ff320c635bda208d46500cdf8c24e5024bc92bd0e4a8f249cb489218

contract ERC721HashRestore {
    function hashes(uint256 id0, uint256 id1) external pure returns (bytes32 first, bytes32 second) {
        assembly {
            mstore(0, id0)
            mstore(28, shl(192, 0x7d8825530a5a2e7a))
            first := keccak256(0, 32)
            mstore(0, id1)
            mstore(28, shl(192, 0x7d8825530a5a2e7a))
            second := keccak256(0, 32)
        }
    }
}
