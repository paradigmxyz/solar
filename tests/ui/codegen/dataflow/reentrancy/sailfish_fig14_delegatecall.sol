//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Fig. 14: delegatecall-based reentrancy. `funcA` sets the flag and delegate-calls
// attacker-supplied data into this contract, which can reach `funcC`'s untrusted callback and
// re-enter `funcB` while the flag it asserts is still set.

// CHECK: fn @funcA:
// CHECK: address_delegatecall {{.*}}; delegatecall to this
// CHECK: finding: reentrancy delegatecall @funcA call#{{[0-9.]+}}: writes slot(0) after the call; reentrant @funcB reads slot(0) (stale read)
interface Receiver {
    function tokenFallback(address from, uint256 value, bytes calldata data) external;
}

contract DelegateReentrancy {
    bool __isTokenFallback;
    uint256 appData;

    function funcA(bytes memory _data) public {
        __isTokenFallback = true;
        (bool ok, ) = address(this).delegatecall(_data);
        //~^ WARN: possible delegatecall reentrancy: `funcA` writes storage after an external call
        ok;
        __isTokenFallback = false;
        //~^ NOTE: `funcA` writes `slot(0)` after the call
    }

    function funcB() public {
        assert(__isTokenFallback);
        //~^ NOTE: a reentrant call to `funcB` can read it here (stale read)
        appData += 1;
    }

    function funcC(address _to) public {
        Receiver receiver = Receiver(_to);
        receiver.tokenFallback(msg.sender, 0, "");
    }
}
