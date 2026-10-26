//@ codegen-matrix: standard
//@ run-call: run => 133

// Both pointers have the same word layout. Their element reference shapes still need distinct
// dispatchers, and ordinary typed calls must remain usable inside a scratch block.
contract Test {
    function bytesLength(bytes[] memory values) internal pure returns (uint256) {
        return values[0].length;
    }

    function firstWord(uint256[] memory values) internal pure returns (uint256) {
        return values[0];
    }

    function run() external pure returns (uint256 result) {
        function(bytes[] memory) internal pure returns (uint256) bytesReader = bytesLength;
        function(uint256[] memory) internal pure returns (uint256) wordReader = firstWord;
        /// @custom:solar-scratch
        {
            bytes[] memory nested = new bytes[](1);
            nested[0] = abi.encode(uint256(7));
            uint256[] memory words = new uint256[](1);
            words[0] = 101;
            result = bytesReader(nested) + wordReader(words);
        }
    }
}
