// The records of `@custom:solar-fuse` groups appear in the storage layout under
// `fused`, keyed by the slot of each group's first mapping, with each member's
// slot relative to the record. Each mapping keeps its own entry in `storage`.
contract Accounts {
    uint256 total;

    /// @custom:solar-fuse account
    mapping(address => uint128) balance;
    /// @custom:solar-fuse account
    mapping(address => uint64) nonce;
    /// @custom:solar-fuse account
    mapping(address => uint64) expiry;

    /// @custom:solar-fuse order
    mapping(uint256 => address) owner;
    /// @custom:solar-fuse order
    mapping(uint256 => uint256) amount;
    /// @custom:solar-fuse order
    mapping(uint256 => bool) open;
}

contract Plain {
    mapping(address => uint256) balance;
}
