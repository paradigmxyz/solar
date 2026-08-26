//@ codegen-matrix: standard
//@ run-call: Positions::run() => 5, 5, 12

// Storage references returned through named returns, and mappings nested in
// structs reached through runtime storage-reference slots.

contract Positions {
    struct Position {
        uint128 liquidity;
        uint256 feeGrowth0;
    }

    struct State {
        uint256 head;
        mapping(bytes32 => Position) positions;
    }

    mapping(bytes32 => State) internal pools;

    function _get(mapping(bytes32 => Position) storage self, bytes32 key)
        internal
        view
        returns (Position storage position)
    {
        position = self[key];
    }

    function _getPool(bytes32 id) internal view returns (State storage) {
        return pools[id];
    }

    function run() external returns (uint256, uint256, uint256) {
        bytes32 id = keccak256("pool");
        bytes32 pk = keccak256("position");

        State storage pool = _getPool(id);
        Position storage pos = _get(pool.positions, pk);
        pos.liquidity += 5;

        uint256 direct = pools[id].positions[pk].liquidity;
        uint256 through = _get(_getPool(id).positions, pk).liquidity;

        Position storage again = _get(pool.positions, pk);
        again.feeGrowth0 = 12;

        return (direct, through, pools[id].positions[pk].feeGrowth0);
    }
}
