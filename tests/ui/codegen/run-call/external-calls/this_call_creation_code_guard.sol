//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call-fail: Deployer::viaHelper => 0x
//@ run-call-fail: Deployer::direct => 0x
//@ run-call-fail: Deployer::viaModifier => 0x
//@ run-call-fail: Deployer::viaBase => 0x
//@ run-call-fail: Deployer::viaInitializer => 0x
//@ run-call-fail: Deployer::viaTry => 0x
//@ run-call-fail: Deployer::viaShared => 0x
//@ run-call: Runtime::viaHelper => 1
//@ run-call: SharedHelper::viaHelper; constructor=[false] => 1

// A `this.f()` call keeps the `extcodesize` guard wherever solc emits one, so creation code is
// guarded too: the contract's code is only stored once the constructor returns, and the call
// would otherwise silently do nothing. A function the runtime object shares with the creation
// object has one body, so it keeps the guard in both copies; the runtime call still succeeds
// because the code exists by then.

// CHECK-LABEL: @module CreationHelper
// CHECK-LABEL: fn @helper
// CHECK: extcodesize
// CHECK: call
contract CreationHelper {
    uint256 public seen;

    constructor() {
        helper();
    }

    function helper() internal {
        this.setIt();
    }

    function setIt() external {
        seen = seen + 1;
    }
}

// CHECK-LABEL: @module CreationDirect
// CHECK-LABEL: fn @constructor
// CHECK: extcodesize
// CHECK: call
contract CreationDirect {
    uint256 public seen;

    constructor() {
        this.setIt();
    }

    function setIt() external {
        seen = seen + 1;
    }
}

// CHECK-LABEL: @module CreationModifier
// CHECK-LABEL: fn @constructor
// CHECK: extcodesize
// CHECK: call
contract CreationModifier {
    uint256 public seen;

    modifier m() {
        this.setIt();
        _;
    }

    constructor() m() {}

    function setIt() external {
        seen = seen + 1;
    }
}

contract CreationBase {
    uint256 public seen;

    constructor() {
        this.setIt();
    }

    function setIt() external {
        seen = seen + 1;
    }
}

// CHECK-LABEL: @module CreationDerived
// CHECK-LABEL: fn @constructor
// CHECK: extcodesize
// CHECK: call
contract CreationDerived is CreationBase {
    constructor() CreationBase() {}
}

// A state variable initializer runs in creation code as well, so its callees keep the guard.
// CHECK-LABEL: @module CreationInitializer
// CHECK-LABEL: fn @init
// CHECK: extcodesize
// CHECK: call
contract CreationInitializer {
    uint256 public seen;
    uint256 public x = init();

    function init() internal returns (uint256) {
        this.setIt();
        return 3;
    }

    function setIt() external {
        seen = seen + 1;
    }
}

// The `try` form of the same call keeps the guard too. The guard reverts before the call, so the
// `catch` clause does not run and the deployment fails.
// CHECK-LABEL: @module CreationTry
// CHECK-LABEL: fn @constructor
// CHECK: extcodesize
// CHECK: call
contract CreationTry {
    uint256 public seen;

    constructor() {
        try this.setIt() {} catch {}
    }

    function setIt() external {
        seen = seen + 1;
    }
}

// The same helper in runtime code is guarded as well: the running code is the contract's own only
// until a `DELEGATECALL` frame separates it from `address(this)`, which is what
// `this_call_delegated_guard.sol` covers. The call passes here because the code exists.
// CHECK-LABEL: @module Runtime
// CHECK-LABEL: fn @helper
// CHECK: extcodesize
// CHECK: call
// CHECK-LABEL: fn @viaHelper
contract Runtime {
    uint256 public seen;

    function helper() internal {
        this.setIt();
    }

    function viaHelper() external returns (uint256) {
        helper();
        return seen;
    }

    function setIt() external {
        seen = seen + 1;
    }
}

// One helper reached from both objects: its single body keeps the guard, which the constructor
// only reaches when its argument asks for it, and the runtime call passes.
// CHECK-LABEL: @module SharedHelper
// CHECK-LABEL: fn @helper
// CHECK: extcodesize
// CHECK: call
contract SharedHelper {
    uint256 public seen;

    constructor(bool run) {
        if (run) {
            helper();
        }
    }

    function helper() internal {
        this.setIt();
    }

    function viaHelper() external returns (uint256) {
        helper();
        return seen;
    }

    function setIt() external {
        seen = seen + 1;
    }
}

// A failing deployment is only observable from a call, so each case is deployed by this contract.
contract Deployer {
    function viaHelper() external {
        new CreationHelper();
    }

    function direct() external {
        new CreationDirect();
    }

    function viaModifier() external {
        new CreationModifier();
    }

    function viaBase() external {
        new CreationDerived();
    }

    function viaInitializer() external {
        new CreationInitializer();
    }

    function viaTry() external {
        new CreationTry();
    }

    function viaShared() external {
        new SharedHelper(true);
    }
}
