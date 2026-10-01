//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Fig. 3 (Example 2): a mutex refutes the cross-function candidate. At the call
// the mutex is set, so a reentrant `transfer` cannot reach its writes; its read of the mutex
// only makes it return without effects.

// CHECK: fn @withdrawBalance:
// CHECK: address_call {{.*}}; call to caller slot(0)=(entry(slot(0)) & {{.*}} | 0x1)
// CHECK: fn @transfer:
// CHECK: sstore {{.*}}; write slot(1)[caller] if entry(slot(0)) & 0xff == 0
// CHECK-NOT: finding:
contract Mutex {
    bool mutex;
    mapping(address => uint256) userBalance;

    function withdrawBalance(uint256 amount) public {
        if (mutex == false) {
            mutex = true;
            if (userBalance[msg.sender] > amount) {
                (bool ok, ) = msg.sender.call{value: amount}("");
                ok;
                userBalance[msg.sender] -= amount;
            }
            mutex = false;
        }
    }

    function transfer(address to, uint256 amt) public {
        if (mutex == false) {
            mutex = true;
            if (userBalance[msg.sender] > amt) {
                userBalance[to] += amt;
                userBalance[msg.sender] -= amt;
            }
            mutex = false;
        }
    }
}
