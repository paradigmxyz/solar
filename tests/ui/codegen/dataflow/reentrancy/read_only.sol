//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Read-only reentrancy: during the payout the pool's assets are already reduced but its
// shares are not, so a view that prices shares returns a stale ratio to any contract that
// the recipient calls, even when every state-changing function is guarded.

// CHECK: finding: reentrancy read-only @withdraw {{.*}} writes slot(1) after the call; reentrant @price reads slot(1) (stale read)
contract Pool {
    bool locked;
    uint256 totalShares;
    uint256 totalAssets;
    mapping(address => uint256) shares;

    modifier nonReentrant() {
        require(!locked);
        locked = true;
        _;
        locked = false;
    }

    function withdraw(uint256 amount) external nonReentrant {
        uint256 assets = amount * totalAssets / totalShares;
        totalAssets -= assets;
        (bool ok, ) = msg.sender.call{value: assets}("");
        //~^ WARN: possible read-only reentrancy: `withdraw` writes storage after an external call
        require(ok);
        shares[msg.sender] -= amount;
        totalShares -= amount;
        //~^ NOTE: `withdraw` writes `slot(1)` after the call
    }

    function price() external view returns (uint256) {
        return totalAssets * 1e18 / totalShares;
        //~^ NOTE: a reentrant call to `price` can read it here (stale read)
    }
}
