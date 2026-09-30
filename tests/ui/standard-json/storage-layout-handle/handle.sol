// A struct documented `@custom:solar-handle` lists each handle field as `uint72`
// among its members, and the storage layout lists the field's dictionary under
// `handles`, keyed by the struct's type.
contract Orders {
    /// @custom:solar-handle marketId markets
    struct Order {
        address maker;
        bytes32 marketId;
        uint128 amount;
        uint128 price;
    }

    bytes32[] markets;
    mapping(uint256 => Order) orders;
}

contract Plain {
    struct Order {
        address maker;
        bytes32 marketId;
    }

    Order order;
}
