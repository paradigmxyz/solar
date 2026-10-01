//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// OpenZeppelin's `ReentrancyGuard` sets and checks the lock inside internal helpers. Their
// summaries leave `_status == ENTERED` at the call and require `_status != ENTERED` on
// every committing path of a guarded entry, so reentering `withdraw` or `deposit` is
// infeasible. The unguarded `sweep` stays reachable and is reported.

// CHECK-LABEL: :Vault ===
// CHECK: fn @withdraw:
// CHECK: call {{.*}}; call to immutable0 slot(0)=2 slot(2)=? if entry(slot(0)) != 2
// CHECK: exit: slot(0)=1 requires entry(slot(0)) != 2
// CHECK: fn @_nonReentrantBefore:
// CHECK: exit: slot(0)=2 requires entry(slot(0)) != 2
// CHECK: finding: reentrancy cross-function @withdraw {{.*}} writes slot(2) after the call; reentrant @sweep reads slot(2) (stale read)
// CHECK-NOT: finding:
abstract contract ReentrancyGuard {
    uint256 private constant NOT_ENTERED = 1;
    uint256 private constant ENTERED = 2;
    uint256 private _status;

    error ReentrancyGuardReentrantCall();

    constructor() { _status = NOT_ENTERED; }

    modifier nonReentrant() {
        _nonReentrantBefore();
        _;
        _nonReentrantAfter();
    }

    function _nonReentrantBefore() private {
        if (_status == ENTERED) revert ReentrancyGuardReentrantCall();
        _status = ENTERED;
    }

    function _nonReentrantAfter() private { _status = NOT_ENTERED; }
}

interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
}

contract Vault is ReentrancyGuard {
    mapping(address => uint256) balances;
    uint256 pending;
    IERC20 immutable token;

    constructor(IERC20 t) { token = t; }

    function withdraw(uint256 amount) external nonReentrant {
        balances[msg.sender] -= amount;
        pending += amount;
        token.transfer(msg.sender, amount);
        //~^ WARN: possible cross-function reentrancy: `withdraw` writes storage after an external call
        pending -= amount;
        //~^ NOTE: `withdraw` writes `slot(2)` after the call
    }

    function deposit(uint256 amount) external nonReentrant {
        balances[msg.sender] += amount;
    }

    function sweep() external {
        if (pending == 0) balances[address(this)] = 0;
        //~^ NOTE: a reentrant call to `sweep` can read it here (stale read)
    }
}
