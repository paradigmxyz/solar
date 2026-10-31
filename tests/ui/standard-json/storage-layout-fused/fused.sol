// The records of `@custom:solar-storage fuse` groups appear in the storage layout under
// `fused`, keyed by the slot of each group's first mapping, with each member's
// slot relative to the record. Each mapping keeps its own entry in `storage`.
contract Accounts {
    uint256 total;

    /// @custom:solar-storage fuse account
    mapping(address => uint128) balance;
    /// @custom:solar-storage fuse account
    mapping(address => uint64) nonce;
    /// @custom:solar-storage fuse account
    mapping(address => uint64) expiry;

    /// @custom:solar-storage fuse order
    mapping(uint256 => address) owner;
    /// @custom:solar-storage fuse order
    mapping(uint256 => uint256) amount;
    /// @custom:solar-storage fuse order
    mapping(uint256 => bool) open;
}

contract Plain {
    mapping(address => uint256) balance;
}
