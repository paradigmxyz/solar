// `@custom:solar-handle <field> <dictionary>` keeps, in place of a field's value, the index of
// the value in a dictionary. The field must fill a word, the dictionary must be a storage array
// of the field's type in the struct's contract, the field may only be set to an element of the
// dictionary or to zero, and the dictionary may only grow.
library Words {
    function sum(bytes32[] storage words) internal view returns (bytes32 total) {
        for (uint256 i; i < words.length; ++i) total ^= words[i];
    }

    function first(Book.Order storage order) internal view returns (bytes32) {
        //~^ ERROR: a struct with handles can only be stored by its contract and the contracts that inherit it
        return order.marketId;
    }
}

/// @custom:solar-handle id ids
//~^ ERROR: `@custom:solar-handle` must document a struct declared in a contract
struct Free {
    bytes32 id;
}

contract Book {
    using Words for bytes32[];

    /// @custom:solar-handle marketId markets
    struct Order {
        address maker;
        bytes32 marketId;
        uint128 amount;
    }

    /// @custom:solar-handle
    //~^ ERROR: `@custom:solar-handle` must name a field and its dictionary
    /// @custom:solar-handle id
    //~^ ERROR: `@custom:solar-handle` must name a field and its dictionary
    /// @custom:solar-handle id markets extra
    //~^ ERROR: `@custom:solar-handle` must name a field and its dictionary
    /// @custom:solar-handle missing markets
    //~^ ERROR: struct `Bad` has no field `missing`
    /// @custom:solar-handle small markets
    //~^ ERROR: a handle field must be a value type that fills a word
    /// @custom:solar-handle id missing
    //~^ ERROR: contract `Book` has no state variable `missing`
    /// @custom:solar-handle id amounts
    //~^ ERROR: a handle dictionary must be a storage array of `bytes32`
    /// @custom:solar-handle id pair
    //~^ ERROR: a handle dictionary must be a storage array of `bytes32`
    struct Bad {
        bytes32 id;
        uint64 small;
    }

    /// @custom:solar-handle id markets
    /// @custom:solar-handle id markets
    //~^ ERROR: field `id` has more than one handle tag
    struct Twice {
        bytes32 id;
    }

    /// @custom:solar-handle size sizes
    struct Sized {
        uint256 size;
    }

    bytes32[] markets;
    bytes32[] other;
    uint256[] amounts;
    bytes32[2] pair;
    uint256[] sizes;
    mapping(uint256 => Order) orders;
    Order[] list;
    Sized sized;

    Order initialized = Order(address(0), bytes32(0), 0);
    //~^ ERROR: a struct with handles cannot be written to storage as a whole

    /// @custom:solar-handle marketId markets
    //~^ ERROR: `@custom:solar-handle` must document a struct declared in a contract
    function misplaced() internal {}

    function allowed(Order memory m, bytes32 value) internal {
        Order storage o = orders[1];
        o.marketId = markets[0];
        o.marketId = 0;
        o.marketId = bytes32(0);
        delete o.marketId;
        o = orders[2];
        m.marketId = value;
        orders[3].marketId = markets[m.amount];
        list.push();
        list[0].marketId = markets[1];
        delete orders[4];
        markets.push(value);
        markets.push();
        bytes32[] memory copy = markets;
        m = orders[5];
        sized.size = sizes[0];
        delete sized;
        copy[0] = markets[markets.length - 1];
    }

    function values(Order memory m, bytes32 value) internal {
        orders[1].marketId = value; //~ ERROR: a handle field can only be set to an element of `markets` or to zero
        orders[1].marketId = other[0]; //~ ERROR: a handle field can only be set to an element of `markets` or to zero
        orders[1].marketId = m.marketId; //~ ERROR: a handle field can only be set to an element of `markets` or to zero
        orders[1].marketId |= markets[0]; //~ ERROR: a handle field can only be set to an element of `markets` or to zero
        sized.size++; //~ ERROR: a handle field can only be set to an element of `sizes` or to zero
        sized.size += 1; //~ ERROR: a handle field can only be set to an element of `sizes` or to zero
        (orders[1].marketId, value) = (markets[0], value); //~ ERROR: a tuple assignment cannot write handle fields
    }

    function wholes(Order memory m) internal {
        orders[1] = m; //~ ERROR: a struct with handles cannot be written to storage as a whole
        orders[1] = orders[2]; //~ ERROR: a struct with handles cannot be written to storage as a whole
        list[0] = m; //~ ERROR: a struct with handles cannot be written to storage as a whole
        list.push(m); //~ ERROR: a struct with handles cannot be written to storage as a whole
        (orders[1], m) = (m, m); //~ ERROR: a tuple assignment cannot write handle fields
    }

    function dictionary(bytes32 value, bool flag) internal {
        markets.pop(); //~ ERROR: a handle dictionary is append-only
        markets[0] = value; //~ ERROR: a handle dictionary is append-only
        delete markets[0]; //~ ERROR: a handle dictionary is append-only
        delete markets; //~ ERROR: a handle dictionary is append-only
        markets = other; //~ ERROR: a handle dictionary is append-only
        markets.push() = value; //~ ERROR: a handle dictionary is append-only
        (markets[0], value) = (value, value); //~ ERROR: a handle dictionary is append-only
        bytes32[] storage r = markets; //~ ERROR: a handle dictionary cannot be a storage reference
        r = markets; //~ ERROR: a handle dictionary cannot be a storage reference
        Words.sum(markets); //~ ERROR: a handle dictionary cannot be a storage reference
        markets.sum(); //~ ERROR: a handle dictionary has no member `sum` this compiler keeps
        (flag ? markets : other).push(value); //~ ERROR: a handle dictionary cannot be chosen by a conditional expression
    }

    function assembly_() internal view {
        assembly {
            let s := markets.slot //~ ERROR: inline assembly cannot take the `.slot` of a handle dictionary
            let t := orders.slot //~ ERROR: inline assembly cannot take the `.slot` of a storage value that holds a struct with handles
        }
        Order storage o = orders[1];
        assembly {
            let u := o.slot //~ ERROR: inline assembly cannot take the `.slot` of a storage value that holds a struct with handles
        }
    }
}

contract Other {
    Book.Order kept; //~ ERROR: a struct with handles can only be stored by its contract and the contracts that inherit it
}

contract Derived is Book {
    Order extra;

    function set() external {
        extra.marketId = markets[0];
    }
}

function free(Book.Order storage order) view returns (bytes32) {
    //~^ ERROR: a struct with handles can only be stored by its contract and the contracts that inherit it
    return order.marketId;
}
