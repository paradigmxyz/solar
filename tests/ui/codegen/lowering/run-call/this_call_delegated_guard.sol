//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call-fail: Deployer::deployProxy => Error("init failed")
//@ run-call: Deployer::callImplDirectly => 1

// A `this.f()` call keeps its `extcodesize` guard wherever solc emits one, which is wherever the
// call expects no return data or the version has no `RETURNDATASIZE`. The executing code and
// `address(this)` come apart under `DELEGATECALL`: here the implementation's `init` runs on a
// proxy that is still in its constructor and has no code, so `this.ping()` reaches a code-less
// account. Without the guard the `CALL` would succeed without running `ping`, and the
// initialization would be silently skipped.

// CHECK-LABEL: @module Impl
// CHECK-LABEL: fn @init
// CHECK: extcodesize
// CHECK: call
contract Impl {
    uint256 public initialized;

    function init() external {
        this.ping();
        initialized = 1;
    }

    function ping() external {}
}

contract Proxy {
    uint256 public initialized;

    constructor(address impl) {
        (bool ok, ) = impl.delegatecall(abi.encodeWithSelector(Impl.init.selector));
        require(ok, "init failed");
    }
}

// A failing deployment is only observable from a call, so the proxy is deployed by this contract.
// The same `init` called on the implementation itself finds the code it is running and succeeds.
contract Deployer {
    function deployProxy() external returns (uint256) {
        Impl impl = new Impl();
        Proxy proxy = new Proxy(address(impl));
        return proxy.initialized();
    }

    function callImplDirectly() external returns (uint256) {
        Impl impl = new Impl();
        impl.init();
        return impl.initialized();
    }
}
