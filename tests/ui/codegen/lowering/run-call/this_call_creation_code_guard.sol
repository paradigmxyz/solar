//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call-fail: Deployer::viaHelper => 0x
//@ run-call-fail: Deployer::direct => 0x
//@ run-call-fail: Deployer::viaModifier => 0x
//@ run-call-fail: Deployer::viaBase => 0x
//@ run-call: Runtime::viaHelper => 1

// A `this.f()` call in creation code has to keep the `extcodesize` guard: the contract's code is
// only stored once the constructor returns, so the call would otherwise silently do nothing.
// Every function copied into the creation object keeps the guard, not just the constructor.

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

// The same helper in runtime code needs no guard: the running code is the contract's own.
// CHECK-LABEL: @module Runtime
// CHECK-LABEL: fn @helper
// CHECK-NOT: extcodesize
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
}
