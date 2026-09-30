//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, App. II-A: the CREAM Finance/AMP exploit, simplified. `borrow` sends ERC777-style
// tokens whose transfer hook calls the receiver, and records the debt only afterwards, so a
// reentrant `borrow` sees the stale debt. The token's address is trusted, but its code still
// calls back.

// CHECK: fn @borrow:
// CHECK: call {{.*}}; call to immutable0
// CHECK: finding: reentrancy {{.*}} @borrow {{.*}} writes slot(0)[caller] after the call; reentrant @borrow reads slot(0)[caller] (stale read)
interface IHookToken {
    function transfer(address to, uint256 amount) external returns (bool);
}

contract CreamMarket {
    mapping(address => uint256) borrowed;
    mapping(address => uint256) collateral;
    IHookToken immutable amp;

    constructor(IHookToken token) { amp = token; }

    function borrow(uint256 amount) external {
        require(borrowed[msg.sender] + amount <= collateral[msg.sender]);
        //~^ NOTE: a reentrant call to `borrow` can read it here (stale read)
        amp.transfer(msg.sender, amount);
        //~^ WARN: possible single-function reentrancy: `borrow` writes storage after an external call
        borrowed[msg.sender] += amount;
        //~^ NOTE: `borrow` writes `slot(0)[caller]` after the call
    }

    function deposit() external payable {
        collateral[msg.sender] += msg.value;
    }
}
