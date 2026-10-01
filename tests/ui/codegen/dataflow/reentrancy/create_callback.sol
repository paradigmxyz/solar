//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Create-based reentrancy: the created contract's constructor calls back into its creator
// before `spawn` records the new child, so a reentrant `spawn` passes the limit check again
// and the factory creates more children than it allows. Deferred child bytecode is opaque
// at MIR analysis time. QuietFactory is a documented false positive until callback-free
// constructor/runtime provenance can be proved without inspecting a partial initcode prefix.

// CHECK-LABEL: :Factory ===
// CHECK: create {{.*}}; create of code that may call out
// CHECK: finding: reentrancy contract creation @spawn {{.*}} writes slot(0) after the call; reentrant @spawn reads slot(0) (stale read)
// CHECK-LABEL: :QuietFactory ===
// CHECK: create {{.*}}; create of code that may call out
// CHECK: finding: reentrancy contract creation @spawn
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
        //~^ WARN: possible contract creation reentrancy: `spawn` writes storage after an external call
        children += 1;
    }
}
