// A struct documented with `@custom:storage-location erc7201:<id>` lives at the
// location ERC-7201 derives from the id. An accessor points a storage reference
// there in assembly, so a constant it assigns to `.slot` must be that location,
// and no contract may see two structs in one namespace.
contract Token {
    /// @custom:storage-location erc7201:openzeppelin.storage.ERC20
    struct ERC20Storage {
        mapping(address => uint256) balances;
        uint256 totalSupply;
    }

    bytes32 private constant ERC20_LOCATION =
        0x52c63247e1f47db19d5ce0460030c497f067ca4cebf71ba98eeadabe20bace00;
    bytes32 private constant WRONG_LOCATION =
        0x52c63247e1f47db19d5ce0460030c497f067ca4cebf71ba98eeadabe20bace01;

    function _erc20() private pure returns (ERC20Storage storage $) {
        assembly {
            $.slot := ERC20_LOCATION
        }
    }

    function _wrong() private pure returns (ERC20Storage storage $) {
        assembly {
            $.slot := WRONG_LOCATION //~ ERROR: this is not the storage location of ERC-7201 namespace `openzeppelin.storage.ERC20`
        }
    }

    function _literal() private pure returns (ERC20Storage storage $) {
        assembly {
            $.slot := 0 //~ ERROR: this is not the storage location of ERC-7201 namespace `openzeppelin.storage.ERC20`
        }
    }

    // A slot computed at run time is not checked.
    function _dynamic(bytes32 location) private pure returns (ERC20Storage storage $) {
        assembly {
            $.slot := location
        }
    }

    function total(bytes32 location) external view returns (uint256) {
        return _erc20().totalSupply + _wrong().totalSupply + _literal().totalSupply
            + _dynamic(location).totalSupply;
    }
}

contract Base {
    /// @custom:storage-location erc7201:example.main
    struct MainStorage {
        uint256 x;
    }
}

contract Derived is Base {
    //~v ERROR: ERC-7201 namespace `example.main` is declared twice
    /// @custom:storage-location erc7201:example.main
    struct OtherStorage {
        uint256 y;
    }
}

// Other formulas are not checked.
contract Custom {
    /// @custom:storage-location custom:example.main
    struct CustomStorage {
        uint256 z;
    }
}
