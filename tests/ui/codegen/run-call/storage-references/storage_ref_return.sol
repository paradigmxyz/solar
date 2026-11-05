//@ codegen-matrix: standard
//@ run-call: Pools::run => 96, 7

// An internal function returning a storage reference hands back the slot,
// not a materialized memory copy: the caller reads and writes the referent
// through it.

contract Pools {
    struct State {
        uint160 price;
        uint16 fee;
        uint256 liquidity;
    }

    mapping(bytes32 => State) internal pools;

    function _getPool(bytes32 id) internal view returns (State storage) {
        return pools[id];
    }

    function run() external returns (uint256, uint256) {
        bytes32 id = keccak256("pool");
        pools[id].price = 96;
        pools[id].fee = 30;

        State storage pool = _getPool(id);
        require(pool.price == 96, "read through ref");
        pool.liquidity = 7;

        return (uint256(_getPool(id).price), pools[id].liquidity);
    }
}
