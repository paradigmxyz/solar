pragma solidity ^0.8.20;
// A view a spec calls keeps its own locals: a storage pointer of the same name in the caller does
// not answer for them.
contract SpecViewScope {
    struct Fee {
        address account;
        uint16 rateBps;
    }

    struct Stored {
        Fee[4] fees;
        uint8 count;
    }

    uint256 private total;
    mapping(uint64 => Stored) private configs;

    function totalLessOne() public view returns (uint256) {
        uint256 stored = total;
        return stored == 0 ? 0 : stored - 1;
    }

    /// @custom:requires n <= 4
    /// @custom:ensures totalLessOne() == old(totalLessOne())
    function ok_storage_pointer_named_like_view_local(uint64 id, uint8 n, address account) external {
        Stored storage stored = configs[id];
        /// @custom:invariant i <= n
        /// @custom:invariant totalLessOne() == old(totalLessOne())
        for (uint256 i = 0; i < n; i++) {
            stored.fees[i] = Fee(account, 1);
        }
    }

    /// @custom:ensures totalLessOne() == old(totalLessOne()) + 1
    function bad_storage_pointer_named_like_view_local(uint64 id, address account) external {
        Stored storage stored = configs[id];
        stored.fees[0] = Fee(account, 1);
    }
}
