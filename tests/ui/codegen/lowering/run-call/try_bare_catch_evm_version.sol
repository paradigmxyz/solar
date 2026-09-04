//@ revisions: homestead homesteadGas homesteadSize byzantium osaka
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD --implicit-check-not=returndatasize --implicit-check-not=make_returndata_slice
//@[homesteadGas] compile-flags: -O gas --evm-version homestead
//@[homesteadSize] compile-flags: -O size --evm-version homestead
//@[byzantium] compile-flags: -O none --evm-version byzantium
//@[osaka] compile-flags: -O none --evm-version osaka -Zdump=mir
//@[osaka] filecheck: --check-prefix=OSAKA
//@ run-call-fail: TryBareCatch::noCode => 0x
//@ run-call-fail: TryBareCatch::noCodeNoReturn => 0x
//@ run-call: TryBareCatch::live => 42
//@ run-call: TryBareCatch::liveTwo => 3
//@ run-call: TryBareCatch::liveAggregate => 33
//@ run-call: TryBareCatch::liveNoReturn => 1
//@ run-call: TryBareCatch::liveCatch => 7
//@ run-call: TryBareCatch::livePointer => 42
//@ run-call: TryBareCatch::liveCreation => 1

interface TryTarget {
    function value() external returns (uint256);
    function pair() external returns (uint256, uint256);
    function agg() external returns (uint256[2] memory);
    function noop() external;
    function fail() external payable returns (uint256);
}

contract TryCallee {
    function value() external pure returns (uint256) {
        return 42;
    }

    function pair() external pure returns (uint256, uint256) {
        return (1, 2);
    }

    function agg() external pure returns (uint256[2] memory r) {
        r[0] = 11;
        r[1] = 22;
    }

    function noop() external {}

    function fail() external payable returns (uint256) {
        revert();
    }
}

contract TryBareCatch {
    // Before Byzantium a bare `catch { }` needs no return data: the call writes its return
    // values into an output area of its own and the failure path runs the clause as it is.
    // HOMESTEAD-LABEL: fn @live
    // HOMESTEAD: extcodesize
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: [[OK:v[0-9]+]] = call [[FWD]], {{v[0-9]+}}, 0, {{v[0-9]+}}, {{v[0-9]+}}, 0, 32
    // HOMESTEAD: jumpi [[OK]]
    // OSAKA-LABEL: fn @live
    // OSAKA: [[OK:v[0-9]+]] = call {{v[0-9]+}}, {{v[0-9]+}}, 0, {{v[0-9]+}}, {{v[0-9]+}}, 0, 0
    // OSAKA: jumpi [[OK]]
    // OSAKA: returndatasize
    function live() external returns (uint256 r) {
        try TryTarget(address(new TryCallee())).value() returns (uint256 v) {
            r = v;
        } catch {
            r = 7;
        }
    }

    // A multi-word output area gets a buffer of its own before EIP-150, whose last word is
    // touched before the gas is read.
    // HOMESTEAD-LABEL: fn @liveTwo
    // HOMESTEAD: [[BUFFER:v[0-9]+]] = alloc raw, exact, uninitialized, infallible, 64
    // HOMESTEAD: [[LAST:v[0-9]+]] = add [[BUFFER]], 32
    // HOMESTEAD: mstore [[LAST]], 0
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]], {{v[0-9]+}}, 0, {{v[0-9]+}}, {{v[0-9]+}}, [[BUFFER]], 64
    function liveTwo() external returns (uint256 r) {
        try TryTarget(address(new TryCallee())).pair() returns (uint256 a, uint256 b) {
            r = a + b;
        } catch {
            r = 7;
        }
    }

    function liveAggregate() external returns (uint256 r) {
        try TryTarget(address(new TryCallee())).agg() returns (uint256[2] memory v) {
            r = v[0] + v[1];
        } catch {
            r = 7;
        }
    }

    // A call without return values keeps its `extcodesize` guard at every version.
    // HOMESTEAD-LABEL: fn @liveNoReturn
    // HOMESTEAD: extcodesize
    // HOMESTEAD: call
    // OSAKA-LABEL: fn @liveNoReturn
    // OSAKA: extcodesize
    // OSAKA: call
    function liveNoReturn() external returns (uint256 r) {
        try TryTarget(address(new TryCallee())).noop() {
            r = 1;
        } catch {
            r = 7;
        }
    }

    // A call that cannot be made at all fails without running the callee, which is the one
    // pre-Byzantium failure that leaves the caller enough gas to run the catch clause: every
    // exception there consumes all the gas the call was forwarded.
    // HOMESTEAD-LABEL: fn @liveCatch
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 0x235a
    // HOMESTEAD: [[OK:v[0-9]+]] = call [[FWD]], {{v[0-9]+}}, 1,
    // HOMESTEAD: jumpi [[OK]]
    function liveCatch() external returns (uint256 r) {
        try TryTarget(address(new TryCallee())).fail{value: 1}() returns (uint256 v) {
            r = v;
        } catch {
            r = 7;
        }
    }

    function livePointer() external returns (uint256 r) {
        TryTarget target = TryTarget(address(new TryCallee()));
        function() external returns (uint256) f = target.value;
        try f() returns (uint256 v) {
            r = v;
        } catch {
            r = 7;
        }
    }

    function liveCreation() external returns (uint256 r) {
        try new TryCallee() {
            r = 1;
        } catch {
            r = 7;
        }
    }

    // A code-less callee reverts the whole function instead of running the catch clause: the
    // `extcodesize` guard is what a pre-Byzantium call has in place of a return-data check.
    // HOMESTEAD-LABEL: fn @noCode
    // HOMESTEAD: extcodesize
    function noCode() external returns (uint256 r) {
        try TryTarget(address(0)).value() returns (uint256 v) {
            r = v;
        } catch {
            r = 7;
        }
    }

    // HOMESTEAD-LABEL: fn @noCodeNoReturn
    // HOMESTEAD: extcodesize
    // OSAKA-LABEL: fn @noCodeNoReturn
    // OSAKA: extcodesize
    function noCodeNoReturn() external returns (uint256 r) {
        try TryTarget(address(0)).noop() {
            r = 1;
        } catch {
            r = 7;
        }
    }
}
