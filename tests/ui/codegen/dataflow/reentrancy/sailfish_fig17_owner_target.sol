//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Fig. 17: a false positive of Sailfish. The call target is set only by an
// owner-guarded function, and `balanceOf` is a static call that cannot write state; no view
// function reads `balances`, so a reentrant static call observes nothing stale.

// CHECK: fn @enableBuyBackMode:
// CHECK: exit: {{.*}}requires caller == entry(slot(0))
// CHECK: fn @transfer:
// CHECK: staticcall to sload(slot(1))
// CHECK-NOT: finding:
interface Token {
    function balanceOf(address who) external view returns (uint256);
}

contract EnvientaPreToken {
    address _creator;
    Token bTken;
    mapping(address => uint256) balances;

    constructor() { _creator = msg.sender; }

    function enableBuyBackMode(address _bTken) public {
        require(msg.sender == _creator);
        bTken = Token(_bTken);
    }

    function transfer(address to, uint256 val) public {
        to;
        require(bTken.balanceOf(address(this)) >= val);
        balances[msg.sender] -= val;
    }
}
