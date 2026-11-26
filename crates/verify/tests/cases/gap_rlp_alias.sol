pragma solidity ^0.8.20;
// An RLP-style decoder that writes an item through a second name for it: aliased memory is not
// modelled, so the write is refused rather than lost.
library RlpLike {
    struct Item {
        uint256 len;
        uint256 ptr;
    }

    function shrink(Item memory outer) internal pure returns (uint256) {
        Item memory inner = outer;
        inner.len = 1;
        return outer.len;
    }
}

contract RlpAlias {
    // outer.len reads the 1 written through inner.
    // expect: aliases `outer` and `inner` is written
    /// @custom:ensures r == 5
    function bad_alias(uint256 v) external pure returns (uint256 r) {
        RlpLike.Item memory outer = RlpLike.Item(5, v);
        RlpLike.Item memory inner = outer;
        inner.len = 1;
        r = outer.len;
    }

    // expect: aliases
    /// @custom:ensures r == 5
    function bad_alias_library(uint256 v) external pure returns (uint256 r) {
        r = RlpLike.shrink(RlpLike.Item(5, v));
    }

    /// @custom:ensures r == 1
    function ok_no_alias(uint256 v) external pure returns (uint256 r) {
        RlpLike.Item memory outer = RlpLike.Item(5, v);
        outer.len = 1;
        r = outer.len;
    }
}
