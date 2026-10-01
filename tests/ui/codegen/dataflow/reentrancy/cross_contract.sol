//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Cross-contract reasoning. `Vault` calls a relay it created; the relay's code calls an
// untrusted address, so control can return into `Vault` and the call is a reentrancy
// vector. Deferred bytecode keeps the quiet helper opaque too: QuietVault documents a
// known false positive pending sound callback-free constructor/runtime provenance.

// CHECK-LABEL: :Vault ===
// CHECK: finding: reentrancy cross-contract @withdraw {{.*}} writes slot(0)[caller] after the call
// CHECK-LABEL: :QuietVault ===
// CHECK: call {{.*}}; call to immutable0 slot(1)=1
// CHECK: sstore 1, 0 {{.*}}
// CHECK: exit: slot(1)=0
// CHECK: finding: reentrancy cross-contract @withdraw
interface IHook {
    function ping() external;
}

contract Relay {
    function forward(address to) external { IHook(to).ping(); }
}

contract Vault {
    mapping(address => uint256) balances;
    Relay immutable relay;

    constructor() { relay = new Relay(); }

    function withdraw(address to) external {
        require(balances[msg.sender] > 0);
        relay.forward(to);
        //~^ WARN: possible cross-contract reentrancy: `withdraw` writes storage after an external call
        balances[msg.sender] = 0;
        //~^ NOTE: `withdraw` writes `slot(0)[caller]` after the call
    }

    function deposit() external payable {
        balances[msg.sender] += msg.value;
        //~^ NOTE: a reentrant call to `deposit` can read it here (stale read)
    }
}

contract Counter {
    uint256 count;
    function bump() external { count += 1; }
}

contract QuietVault {
    mapping(address => uint256) balances;
    uint256 lock;
    Counter immutable counter;

    constructor() { counter = new Counter(); }

    function withdraw() external {
        require(lock == 0);
        lock = 1;
        require(balances[msg.sender] > 0);
        counter.bump();
        //~^ WARN: possible cross-contract reentrancy: `withdraw` writes storage after an external call
        balances[msg.sender] = 0;
        lock = 0;
    }

    function deposit() external payable {
        balances[msg.sender] += msg.value;
    }
}
