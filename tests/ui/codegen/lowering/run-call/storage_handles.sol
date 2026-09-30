//@ codegen-matrix: standard
//@ run-call: roundTrip => 0x000000000000000000000000000000000000bEEF, 0x00000000000000000000000000000000000000000000000000000000000000b2, 5, 7, 99, true
//@ run-call: rawRecord => 0x000000000000000000000002000000000000000000000000000000000000beef, 0x0000000000000000000000000000000700000000000000000000000000000005, 0x0000000000000000000000000000000000000000000000010000000000000063, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: zeroHandle => 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: outOfBounds => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: copy => 0x00000000000000000000000000000000000000000000000000000000000000a1, 11, true
//@ run-call: getter => 0x0000000000000000000000000000000000000000, 0x00000000000000000000000000000000000000000000000000000000000000b2, 0, 0, 0, true
//@ run-call: assignmentValue => 0x00000000000000000000000000000000000000000000000000000000000000b2, 0x00000000000000000000000000000000000000000000000000000000000000b2
//@ run-call: arrays => 0x0000000000000000000000000000000000000000000000000000000000000000, 0x00000000000000000000000000000000000000000000000000000000000000b2, 9, 0x00000000000000000000000000000000000000000000000000000000000000a1
//@ run-call: viaReference => 0x00000000000000000000000000000000000000000000000000000000000000b2
//@ run-call: order => 13, 0x00000000000000000000000000000000000000000000000000000000000000b2
//@ run-call: duplicates => 0x00000000000000000000000000000000000000000000000000000000000000a1, 0x00000000000000000000000000000000000000000000000000000000000000a1, 0x0000000000000000000000000000000000000000000000000000000000000003
//@ run-call: viaFunction => 0x00000000000000000000000000000000000000000000000000000000000000a1, 0x00000000000000000000000000000000000000000000000000000000000000a1
//@ run-call: memoryReturn => (0x0000000000000000000000000000000000000000, 0x00000000000000000000000000000000000000000000000000000000000000b2, 4, 0, 0, false)
//@ run-call: Shifted::shifted => 0x00000000000000000000000000000000000000000000000000000000000000c3, 0x0000000000000000000000000000000000000000000000000000000000000001
//@ run-call: tagged => 0x000000000000000000000000000000000000000000000000000000000000001d, 1000, 3, 0x0000000000000000000000000003000000000000000001000000000000000001, 0x0000000000000000000000000000000000000000000000000000000000000000

type Id is bytes32;

// `@custom:solar-handle <field> <dictionary>` keeps, in place of a field's value, one plus the
// index of the value in the dictionary, in 9 bytes, so the field shares a word with others. Reads
// and writes behave as without the tag; raw reads of the struct show the layout.
abstract contract Book {
    /// @custom:solar-handle marketId markets
    struct Order {
        address maker;
        bytes32 marketId;
        uint128 amount;
        uint128 price;
        uint64 expiry;
        bool isBuy;
    }

    struct Level {
        Order best;
        uint256 depth;
    }

    /// @custom:solar-handle id ids
    /// @custom:solar-handle size sizes
    struct Tagged {
        Id id;
        uint256 size;
        uint32 count;
    }

    bytes32[] public markets;
    mapping(uint256 => Order) public orders;
    Order[] list;
    Level level;
    Id[] ids;
    uint256[] sizes;
    Tagged tag;
    uint256 calls;

    function listMarkets() internal {
        markets.push(bytes32(uint256(0xa1)));
        markets.push(bytes32(uint256(0xb2)));
    }

    function fill(Order storage o, uint256 market) internal {
        o.marketId = markets[market];
    }

    function at(uint256 id) internal view returns (Order storage) {
        return orders[id];
    }
}

// The struct and its dictionary are declared in a base contract.
contract Handles is Book {
    function roundTrip() external returns (address, bytes32, uint128, uint128, uint64, bool) {
        listMarkets();
        Order storage o = orders[7];
        o.maker = address(0xbeef);
        o.marketId = markets[1];
        o.amount = 5;
        o.price = 7;
        o.expiry = 99;
        o.isBuy = true;
        Order storage r = orders[7];
        return (r.maker, r.marketId, r.amount, r.price, r.expiry, r.isBuy);
    }

    // The maker and the handle of `markets[1]` share the first word; the order takes three
    // words instead of four.
    function rawRecord() external returns (bytes32 w0, bytes32 w1, bytes32 w2, bytes32 w3) {
        listMarkets();
        orders[7].maker = address(0xbeef);
        orders[7].marketId = markets[1];
        orders[7].amount = 5;
        orders[7].price = 7;
        orders[7].expiry = 99;
        orders[7].isBuy = true;
        bytes32 record = keccak256(abi.encode(uint256(7), uint256(1)));
        assembly {
            w0 := sload(record)
            w1 := sload(add(record, 1))
            w2 := sload(add(record, 2))
            w3 := sload(add(record, 3))
        }
    }

    // A fresh field reads zero, and so does one set to zero or deleted.
    function zeroHandle() external returns (bytes32 fresh, bytes32 zeroed, bytes32 deleted) {
        listMarkets();
        fresh = orders[1].marketId;
        orders[1].marketId = markets[0];
        orders[1].marketId = bytes32(0);
        zeroed = orders[1].marketId;
        orders[1].marketId = markets[0];
        delete orders[1].marketId;
        deleted = orders[1].marketId;
    }

    // An index past the dictionary fails as reading the element does.
    function outOfBounds() external {
        listMarkets();
        orders[1].marketId = markets[2];
    }

    function copy() external returns (bytes32, uint128, bool) {
        listMarkets();
        orders[3].marketId = markets[0];
        orders[3].amount = 11;
        Order memory m = orders[3];
        return (m.marketId, m.amount, keccak256(abi.encode(orders[3])) == keccak256(abi.encode(m)));
    }

    function getter() external returns (address, bytes32, uint128, uint128, uint64, bool) {
        listMarkets();
        orders[4].marketId = markets[1];
        orders[4].isBuy = true;
        return this.orders(4);
    }

    // The assignment's value is the element.
    function assignmentValue() external returns (bytes32 value, bytes32 stored) {
        listMarkets();
        value = (orders[5].marketId = markets[1]);
        stored = orders[5].marketId;
    }

    function arrays() external returns (bytes32, bytes32, uint256, bytes32) {
        listMarkets();
        list.push();
        list.push();
        list[1].marketId = markets[1];
        list[1].amount = 3;
        level.best.marketId = markets[0];
        level.depth = 9;
        Order[] memory all = list;
        bytes32 second = all[1].marketId;
        list.pop();
        return (list[0].marketId, second, level.depth, level.best.marketId);
    }

    function viaReference() external returns (bytes32) {
        listMarkets();
        fill(orders[6], 1);
        return orders[6].marketId;
    }

    function next(uint256 value) internal returns (uint256) {
        calls = calls * 10 + value;
        return value;
    }

    // The element's index is evaluated before the field's place, as for any other assignment.
    function order() external returns (uint256, bytes32) {
        listMarkets();
        orders[next(3)].marketId = markets[next(1)];
        return (calls, orders[3].marketId);
    }

    // Equal elements are different handles to one value.
    function duplicates() external returns (bytes32, bytes32, bytes32 raw) {
        markets.push(bytes32(uint256(0xa1)));
        markets.push(bytes32(uint256(0xa1)));
        markets.push(bytes32(uint256(0xa1)));
        orders[1].marketId = markets[0];
        orders[2].marketId = markets[2];
        bytes32 record = keccak256(abi.encode(uint256(2), uint256(1)));
        assembly {
            raw := shr(160, sload(record))
        }
        return (orders[1].marketId, orders[2].marketId, raw);
    }

    // A function that returns a storage reference reaches the field too.
    function viaFunction() external returns (bytes32, bytes32) {
        listMarkets();
        at(8).marketId = markets[0];
        return (at(8).marketId, orders[8].marketId);
    }

    function memoryReturn() external returns (Order memory) {
        listMarkets();
        orders[9].marketId = markets[1];
        orders[9].amount = 4;
        return orders[9];
    }

    // Handles of a user-defined value type and of `uint256` pack with a `uint32` in one word.
    function tagged() external returns (bytes32, uint256, uint32, bytes32 raw, bytes32 zeroed) {
        ids.push(Id.wrap(bytes32(uint256(0x1d))));
        sizes.push(1000);
        tag.id = ids[0];
        tag.size = sizes[0];
        tag.count = 3;
        // `tag` follows the three-word order of `level` and its depth, and `ids` and `sizes`.
        assembly {
            raw := sload(9)
        }
        Tagged memory t = tag;
        tag.id = Id.wrap(0);
        zeroed = Id.unwrap(tag.id);
        return (Id.unwrap(t.id), t.size, t.count, raw, zeroed);
    }
}

contract Padding {
    uint256 padding;
}

// Here `markets` follows `padding`, so the handles index the dictionary at slot 1.
contract Shifted is Padding, Book {
    function shifted() external returns (bytes32, bytes32 handle) {
        markets.push(bytes32(uint256(0xc3)));
        orders[1].marketId = markets[0];
        bytes32 record = keccak256(abi.encode(uint256(1), uint256(2)));
        assembly {
            handle := shr(160, sload(record))
        }
        return (orders[1].marketId, handle);
    }
}
