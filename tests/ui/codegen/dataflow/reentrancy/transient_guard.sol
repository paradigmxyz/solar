//@ compile-flags: -Zdataflow=reentrancy --evm-version=cancun
//@ filecheck:
// A transient-storage lock, as in OpenZeppelin's `ReentrancyGuardTransient`. The lock slot
// is tracked like a persistent one, so the guarded entries refute each other.

// CHECK: fn @withdraw:
// CHECK: call {{.*}}; call to caller tslot(0)=1
// CHECK-NOT: finding:
// CHECK: finding: tod @withdraw
contract TransientVault {
    mapping(address => uint256) balances;

    modifier nonReentrant() {
        assembly {
            if tload(0) { revert(0, 0) }
            tstore(0, 1)
        }
        _;
        assembly { tstore(0, 0) }
    }

    function withdraw() external nonReentrant {
        uint256 amount = balances[msg.sender];
        (bool ok, ) = msg.sender.call{value: amount}("");
        //~^ WARN: possible transaction-order dependence: the value of a transfer in `withdraw` depends on storage written by `deposit`
        require(ok);
        balances[msg.sender] = 0;
    }

    function deposit() external payable nonReentrant {
        balances[msg.sender] += msg.value;
    }
}
