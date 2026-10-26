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

/// @custom:solar-safe
contract Unchecked {
    function f(uint256 a) external pure returns (uint256) {
        unchecked { return a + 1; }
    }
}

contract Trusting {
    function f(bytes memory b) external pure returns (uint256) {
        return Reviewed.word(b);
    }
}

contract Unnamed {
    /// @custom:solar-trusted
    constructor() {}

    /// @custom:solar-trusted
    fallback() external payable {}

    /// @custom:solar-trusted
    receive() external payable {}
}

contract Recursive {
    struct Node { Node[] children; }

    /// @custom:solar-trusted
    function reviewed(Node memory node) internal pure returns (uint256) {
        return node.children.length;
    }

    function run() external pure returns (uint256) {
        Node memory node;
        return reviewed(node);
    }
}

library PointerFactory {
    /// @custom:solar-trusted
    function pointer() internal pure returns (function() internal pure returns (uint256)) {
        return target;
    }

    function target() internal pure returns (uint256 result) {
        assembly { result := 42 }
    }
}

contract PointerConsumer {
    function run() external pure returns (uint256) { return PointerFactory.pointer()(); }
}
