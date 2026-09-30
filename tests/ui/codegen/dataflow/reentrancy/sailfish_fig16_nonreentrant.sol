//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Fig. 16: a `nonReentrant` modifier refutes the single-function candidate. The
// lock is set at the call, and every path that commits requires it to be clear.

// CHECK: fn @reapFarm:
// CHECK: call {{.*}}; call to arg0 slot(0)=(entry(slot(0)) & {{.*}} | 0x1) if entry(slot(0)) & 0xff == 0
// CHECK: exit: {{.*}}requires entry(slot(0)) & 0xff == 0
// CHECK-NOT: finding:
interface Corn {
    function transfer(address to, uint256 value) external;
}

contract FreeTaxManFarmer {
    struct User { uint256 workDone; }
    bool reentrancy_lock;
    mapping(address => mapping(address => User)) user;

    modifier nonReentrant() {
        require(!reentrancy_lock);
        reentrancy_lock = true;
        _;
        reentrancy_lock = false;
    }

    function reapFarm(address tokn) public nonReentrant {
        require(user[msg.sender][tokn].workDone > 0);
        Corn(tokn).transfer(msg.sender, user[msg.sender][tokn].workDone);
        user[msg.sender][tokn].workDone = 0;
    }
}
