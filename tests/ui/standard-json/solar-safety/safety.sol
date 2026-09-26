// `solarSafety` reports the properties `@custom:solar-safe` can require for
// every contract: whether the code it runs uses no inline assembly, and
// whether all of its arithmetic is checked, outside the reviewed functions it
// lists as trusted.
library Reviewed {
    /// @custom:solar-trusted
    function word(bytes memory b) internal pure returns (uint256 w) {
        assembly { w := mload(add(b, 32)) }
    }
}

contract Clean {
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }
}

contract Assembly {
    function f() external pure returns (uint256 v) {
        assembly { v := 1 }
    }
}

contract Wrapping {
    function f(uint256 a) external pure returns (uint256) {
        unchecked { return a + 1; }
    }
}

contract Trusting {
    function f(bytes memory b) external pure returns (uint256) {
        return Reviewed.word(b);
    }
}
