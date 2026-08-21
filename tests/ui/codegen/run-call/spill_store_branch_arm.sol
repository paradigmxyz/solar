//@ run-call: Router::go() => 1

// A spill store is a path fact: the third return of the second `fetch` was
// live through a branch diamond, its store landed only in the arm the plan
// emitted it in, and paths through the sibling arm reached the later reload
// with the reserved slot never written (reading stale decode scratch). The
// backend must drop a store's guarantee in blocks its emitting block does
// not dominate.

interface IMgr {
    function bal(address) external view returns (uint256);
    function delta(address) external view returns (int256);
    function consume(uint256 x) external;
    function sync(address) external;
    function last() external view returns (uint256);
}

contract Mgr {
    uint256 public last;

    function bal(address) external pure returns (uint256) {
        return 7;
    }

    function delta(address u) external pure returns (int256) {
        return u == address(1) ? int256(-202) : int256(100);
    }

    function consume(uint256 x) external {
        last = x;
    }

    function sync(address) external {}
}

library Settler {
    function settle(IMgr m, address, uint256 amount) internal {
        m.sync(address(0x1234));
        m.consume(amount);
    }
}

contract Router {
    IMgr internal mgr;
    bool internal flagA = true;
    bool internal flagB = false;

    constructor() {
        mgr = IMgr(address(new Mgr()));
    }

    function fetch(address u) internal view returns (uint256 a, uint256 b, int256 d) {
        a = mgr.bal(u);
        b = mgr.bal(address(this));
        d = mgr.delta(u);
    }

    function go() external returns (uint256) {
        (,, int256 dA0) = fetch(address(1));
        (,, int256 dA1) = fetch(address(2));
        require(dA0 == -202, "d0");
        if (flagA) {
            if (flagB) {
                require(dA1 >= 0, "x");
            } else {
                require(dA1 <= 100, "over");
            }
        }
        if (dA0 < 0) {
            Settler.settle(mgr, msg.sender, uint256(-dA0));
        }
        if (dA1 < 0) {
            Settler.settle(mgr, msg.sender, uint256(-dA1));
        }
        if (dA0 > 0) {
            mgr.consume(uint256(dA0));
        }
        if (dA1 > 0) {
            mgr.consume(uint256(dA1));
        }
        require(mgr.last() == 100, "took wrong amount");
        return 1;
    }
}
