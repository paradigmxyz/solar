//@ codegen-matrix: standard
//@ run-call: f 3 => 10, 0x0000000000000000000000000000000000000007, true
//@ run-call: f 4 => 4, 0x0000000000000000000000000000000000000000, false

// A pure function that only returns a struct literal is inlined at its call
// site. A storage parameter must stay a storage reference there, so a field
// that reads it copies the referenced struct into memory.
contract PureStructConstructorStorageParameter {
    struct Order {
        uint256 amount;
        address maker;
        bool active;
    }

    struct Context {
        uint256 key;
        Order order;
    }

    mapping(uint256 => Order) orders;

    constructor() {
        orders[3] = Order(7, address(7), true);
    }

    function build(uint256 key, Order storage order) internal pure returns (Context memory) {
        return Context({key: key, order: order});
    }

    function f(uint256 key) external view returns (uint256, address, bool) {
        Context memory context = build(key, orders[key]);
        return (context.key + context.order.amount, context.order.maker, context.order.active);
    }
}
