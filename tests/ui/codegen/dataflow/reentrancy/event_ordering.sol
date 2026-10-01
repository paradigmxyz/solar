//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Event ordering: `claim` emits its event after an untrusted call, so a reentrant `claim`
// emits first and off-chain consumers observe the events out of order.

// CHECK: finding: event-ordering @claim {{.*}}: reentrant @claim emits events
contract Rewards {
    event Claimed(address who, uint256 amount);

    function claim(uint256 amount) external {
        (bool ok, ) = msg.sender.call{value: amount}("");
        //~^ WARN: possible event reordering: `claim` emits an event after an external call
        require(ok);
        emit Claimed(msg.sender, amount);
    }
}
