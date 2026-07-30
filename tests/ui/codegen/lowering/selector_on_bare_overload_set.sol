//@compile-flags: -Zcodegen --emit=bin-runtime
// ported-from: test/foundry/utils/TestTokenMinter.sol

// `.selector` on a bare function name inherited through an override chain.
// Name resolution hands back every declaration of the name without
// accounting for overloading, and the type checker disambiguates only
// callees, so this arrives as a multi-candidate set; the candidates share a
// signature and therefore one selector. A set that genuinely disagreed
// would still be reported, as Solidity rejects it too.
abstract contract ERC1155TokenReceiver {
    function onERC1155Received(address, address, uint256, uint256, bytes calldata)
        public virtual returns (bytes4)
    {
        return 0x00000000;
    }
}

contract ERC1155Recipient is ERC1155TokenReceiver {
    function onERC1155Received(address, address, uint256, uint256, bytes calldata)
        public virtual override returns (bytes4)
    {
        return 0x11111111;
    }
}

contract SelectorOnBareOverloadSet is ERC1155Recipient {
    function viaBareIdent() external pure returns (bytes4) {
        return onERC1155Received.selector;
    }
}
