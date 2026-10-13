// The ERC-7201 namespaces a contract declares or inherits appear in its storage
// layout, keyed `erc7201:<id>` with slots relative to each namespace.
contract Base {
    /// @custom:storage-location erc7201:example.main
    struct MainStorage {
        uint256 x;
        uint128 y;
        uint128 z;
        mapping(address => uint256) balances;
    }
}

contract Token is Base {
    uint256 plain;

    /// @custom:storage-location erc7201:example.token
    struct TokenStorage {
        string name;
        address owner;
        bool paused;
    }
}

contract Empty {}
