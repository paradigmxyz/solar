// A non-call reference to a function that appears once per level of the
// inheritance chain (declaration plus overrides) is a single function, so
// `f.selector` resolves; the most derived candidate wins, carrying the
// tightest state mutability. Sets with genuinely different signatures stay
// unresolvable outside calls.

abstract contract TokenReceiver {
    function onERC1155Received(address, address, uint256, uint256, bytes calldata)
        external
        virtual
        returns (bytes4);
}

contract Recipient is TokenReceiver {
    function onERC1155Received(address, address, uint256, uint256, bytes calldata)
        external
        virtual
        override
        returns (bytes4)
    {
        return 0xf23a6e61;
    }
}

contract T is Recipient {
    function selectorOfOverridden() public pure returns (bytes4) {
        return onERC1155Received.selector;
    }
}

contract Overloads {
    function g(uint256 x) public pure returns (uint256) {
        return x;
    }

    function g(uint256 x, uint256 y) public pure returns (uint256) {
        return x + y;
    }

    function bad() public pure returns (bytes4) {
        return g.selector; //~ ERROR: no matching declarations found
    }
}
