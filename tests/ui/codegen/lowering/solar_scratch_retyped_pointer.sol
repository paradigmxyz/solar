//@ compile-flags: -Ogas --emit=bin

// A call through an internal function pointer that inline assembly retyped can
// run a function whose parameters and returns have other types, and the call's
// dispatcher reinterprets each one-word value: a reference to the block's
// memory can leave the block as a plain word, or reach a callee that stores it
// as one. Such a call is rejected, as a callee with inline assembly is.
contract Test {
    uint256 kept;

    function same(bytes memory b) internal pure returns (bytes memory) {
        return b;
    }

    function keep(uint256 word) internal {
        kept = word;
    }

    function returnedAsWord(uint256 x) external pure returns (uint256 word) {
        function(bytes memory) internal pure returns (bytes memory) typed = same;
        function(bytes memory) internal pure returns (uint256) retyped;
        assembly {
            retyped := typed
        }
        /// @custom:solar-scratch
        {
            word = retyped(abi.encode(x)); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
    }

    function storedAsWord(uint256 x) external {
        function(uint256) internal typed = keep;
        function(bytes memory) internal retyped;
        assembly {
            retyped := typed
        }
        /// @custom:solar-scratch
        {
            retyped(abi.encode(x)); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
    }
}

// A retyped pointer whose functions take and return only words other than
// references passes no reference on.
contract Words {
    function half(uint256 x) internal pure returns (uint256) {
        return x / 2;
    }

    function wordsOnly(uint256 x) external pure returns (uint256 y) {
        function(uint256) internal pure returns (uint256) typed = half;
        function(uint8) internal pure returns (uint256) retyped;
        assembly {
            retyped := typed
        }
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(x);
            y = retyped(uint8(encoded.length));
        }
    }
}
