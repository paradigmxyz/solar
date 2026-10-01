//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish (Bose et al., S&P 2022), Fig. 1a: single-function reentrancy (stale read).
// The balance is written after the call, so a reentrant `withdraw` reads the stale value.

// CHECK: fn @withdraw:
// CHECK: address_call {{.*}}; call to caller
// CHECK: sstore {{.*}}; write slot(0)[caller] after call#[[CALL:[0-9.]+]]
// CHECK: finding: reentrancy single-function @withdraw call#[[CALL]]: writes slot(0)[caller] after the call; reentrant @withdraw reads slot(0)[caller] (stale read)
contract Bank {
    mapping(address => uint256) accounts;

    function withdraw(uint256 amount) public {
        if (accounts[msg.sender] >= amount) {
            //~^ NOTE: a reentrant call to `withdraw` can read it here (stale read)
            (bool ok, ) = msg.sender.call{value: amount}("");
            //~^ WARN: possible single-function reentrancy: `withdraw` writes storage after an external call
            ok;
            unchecked { accounts[msg.sender] -= amount; }
            //~^ NOTE: `withdraw` writes `slot(0)[caller]` after the call
        }
    }
}
