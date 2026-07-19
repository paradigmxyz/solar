contract Base {
    uint8 base;

    /// @notice Returns x
    /// @dev Base function
    /// @param x The input
    /// @return y The output
    function f(uint x) public pure virtual returns (uint y) {
        return x;
    }
}

/// @author Alice
/// @title Counter
/// @dev Tracks the counter
/// @custom:security contact security@example.com
contract A is Base layout at 10 {
    struct Pair {
        uint128 left;
        uint128 right;
    }

    /// @inheritdoc Base
    function f(uint x) public pure override returns (uint y) {
        return x;
    }

    /// @notice Returns the count
    /// @dev The stored count
    /// @return count value
    uint public count;

    Pair pair;
    mapping(uint => address) owners;
    uint128 transient low;
    uint128 transient high;
    address transient holder;
}
