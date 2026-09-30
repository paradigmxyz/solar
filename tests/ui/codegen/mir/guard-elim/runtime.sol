//@ codegen-matrix: standard opt
//@ run-call: deposit 5 => 5
//@ run-call: isLocked => false
//@ run-call: bumpAndRead => 1
//@ run-call-fail: reenterSelf => Error("locked")
//@[opt] compile-flags: -Ogas -Zdump=mir -Zdataflow-optimizations
//@[opt] filecheck: --check-prefix=MIR
// Created contracts remain opaque until deployed runtime provenance is proved. Reads of
// the lock see the marker, and a guarded callback still reverts when re-entered.

// MIR-LABEL: fn @deposit(
// MIR: sstore 0, 0
// MIR: ret
// MIR-LABEL: fn @reenterSelf(
// MIR: sstore 0, 0
contract Counter {
    uint256 count;

    function bump() external returns (uint256) {
        count += 1;
        return count;
    }
}

contract Guarded {
    uint256 private unlocked = 1;
    mapping(address => uint256) balances;
    Counter immutable counter;

    modifier lock() {
        require(unlocked == 1, "locked");
        unlocked = 0;
        _;
        unlocked = 1;
    }

    constructor() {
        counter = new Counter();
    }

    function deposit(uint256 amount) external lock returns (uint256) {
        balances[msg.sender] += amount;
        counter.bump();
        return balances[msg.sender] + unlocked;
    }

    function isLocked() external view returns (bool) {
        return unlocked == 0;
    }

    function bumpAndRead() external lock returns (uint256) {
        return counter.bump();
    }

    function reenterSelf() external lock {
        this.deposit(1);
    }
}
