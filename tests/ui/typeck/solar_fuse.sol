// `@custom:solar-storage fuse <group>` keeps the values a group of mapping state
// variables holds for one key in one record. The mappings of a group share a
// key type and map to value types, and each may only be indexed, since a
// storage reference or an assembly `.slot` would reach the standard layout.
contract Fuse {
    /// @custom:solar-storage fuse
    //~^ ERROR: `@custom:solar-storage fuse` must name the group of its mapping
    mapping(address => uint256) unnamed;

    /// @custom:solar-storage fuse single
    //~^ ERROR: fused group `single` has only one mapping
    mapping(address => uint256) alone;

    /// @custom:solar-storage fuse keys
    mapping(address => uint128) byAddress;
    /// @custom:solar-storage fuse keys
    //~^ ERROR: the mappings of fused group `keys` must have the same key type
    mapping(uint256 => uint128) byNumber;

    /// @custom:solar-storage fuse values
    mapping(address => uint128) number;
    /// @custom:solar-storage fuse values
    //~^ ERROR: a fused mapping must map to a value type
    mapping(address => uint256[]) list;

    /// @custom:solar-storage fuse account
    mapping(address => uint128) balance;
    /// @custom:solar-storage fuse account
    mapping(address => uint64) nonce;

    /// @custom:solar-storage fuse account
    //~^ ERROR: `@custom:solar-storage fuse` must document a mapping state variable
    uint256 notMapping;

    function bind() internal view returns (uint128) {
        mapping(address => uint128) storage m = balance; //~ ERROR: a fused mapping can only be indexed
        return m[address(0)];
    }

    function nonceOf(mapping(address => uint64) storage m) internal view returns (uint64) {
        return m[address(0)];
    }

    function passing() external view returns (uint64) {
        return nonceOf(nonce); //~ ERROR: a fused mapping can only be indexed
    }

    function slot() external pure returns (uint256 s) {
        assembly {
            s := balance.slot //~ ERROR: inline assembly cannot take the `.slot` of a fused mapping
        }
    }

    function update(address a) external returns (uint128, uint64) {
        balance[a] += 1;
        delete nonce[a];
        return (balance[a], nonce[a]);
    }
}
