//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Create-based reentrancy: the created contract's constructor calls back into its creator
// before `spawn` records the new child, so a reentrant `spawn` passes the limit check again
// and the factory creates more children than it allows. A child whose code has no call
// instructions cannot call back, so creating it is not a reentrancy vector.

// CHECK-LABEL: :Factory ===
// CHECK: create {{.*}}; create of code that calls out
// CHECK: finding: reentrancy contract creation @spawn {{.*}} writes slot(0) after the call; reentrant @spawn reads slot(0) (stale read)
// CHECK-LABEL: :QuietFactory ===
// CHECK: create {{.*}}; create of code without calls
// CHECK-NOT: finding:
interface IFactory {
    function spawn() external;
}

contract Callback {
    constructor() { IFactory(msg.sender).spawn(); }
}

contract Factory {
    uint256 children;

    function spawn() external {
        require(children < 3);
        //~^ NOTE: a reentrant call to `spawn` can read it here (stale read)
        new Callback();
        //~^ WARN: possible contract creation reentrancy: `spawn` writes storage after an external call
        children += 1;
        //~^ NOTE: `spawn` writes `slot(0)` after the call
    }
}

contract Quiet {
    uint256 value;
    constructor() { value = 1; }
}

contract QuietFactory {
    uint256 children;

    function spawn() external {
        require(children < 3);
        new Quiet();
        children += 1;
    }
}
