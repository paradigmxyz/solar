//@ revisions: homestead tangerineWhistle spuriousDragon byzantium osaka
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD
//@[tangerineWhistle] compile-flags: -O none --evm-version tangerineWhistle
//@[spuriousDragon] compile-flags: -O none --evm-version spuriousDragon
//@[byzantium] compile-flags: -O none --evm-version byzantium
//@[osaka] compile-flags: -O none --evm-version osaka -Zdump=mir
//@[osaka] filecheck: --check-prefix=OSAKA
//@ run-call-fail: NoCodeReturnCalls::direct => 0x
//@ run-call-fail: NoCodeReturnCalls::viewCall => 0x
//@ run-call-fail: NoCodeReturnCalls::pointer => 0x
//@ run-call-fail: NoCodeReturnCalls::aggregate => 0x
//@ run-call-fail: NoCodeReturnCalls::directNoReturn => 0x
//@ run-call-fail: NoCodeReturnCalls::pointerNoReturn => 0x
// Before Tangerine Whistle a call cannot request all the remaining gas, so the
// succeeding cases skip the homestead revision.
//@[tangerineWhistle,spuriousDragon,byzantium,osaka] run-call: NoCodeReturnCalls::live => 42
//@[tangerineWhistle,spuriousDragon,byzantium,osaka] run-call: NoCodeReturnCalls::liveAggregate => 1, 2

interface NoCodeTarget {
    function value() external returns (uint256);
    function noop() external;
}

interface NoCodeViewTarget {
    function value() external view returns (uint256);
}

interface NoCodeAggregateTarget {
    function pair() external returns (uint256[2] memory);
}

contract NoCodeCallee {
    function value() external pure returns (uint256) {
        return 42;
    }

    function pair() external pure returns (uint256[2] memory values) {
        values[0] = 1;
        values[1] = 2;
    }
}

contract NoCodeReturnCalls {
    // A call that expects return data needs no `extcodesize` check from Byzantium on: the
    // return-data length check already reverts for a code-less callee.
    // HOMESTEAD-LABEL: fn @direct
    // HOMESTEAD: extcodesize
    // HOMESTEAD: call
    // HOMESTEAD-NOT: returndatasize
    // OSAKA-LABEL: fn @direct
    // OSAKA-NOT: extcodesize
    // OSAKA: call
    // OSAKA: returndatasize
    // OSAKA-NOT: extcodesize
    function direct() external returns (uint256) {
        return NoCodeTarget(address(0)).value();
    }

    // Pre-Byzantium has no `STATICCALL` either, so the view call is a `CALL`.
    // HOMESTEAD-LABEL: fn @viewCall
    // HOMESTEAD: extcodesize
    // HOMESTEAD: call
    // HOMESTEAD-NOT: staticcall
    // OSAKA-LABEL: fn @viewCall
    // OSAKA-NOT: extcodesize
    // OSAKA: staticcall
    // OSAKA: returndatasize
    function viewCall() external view returns (uint256) {
        return NoCodeViewTarget(address(0)).value();
    }

    // HOMESTEAD-LABEL: fn @pointer
    // HOMESTEAD: extcodesize
    // HOMESTEAD: call
    // OSAKA-LABEL: fn @pointer
    // OSAKA-NOT: extcodesize
    // OSAKA: call
    // OSAKA: returndatasize
    function pointer() external returns (uint256) {
        function() external returns (uint256) target;
        return target();
    }

    // HOMESTEAD-LABEL: fn @aggregate
    // HOMESTEAD: extcodesize
    // HOMESTEAD: call
    // OSAKA-LABEL: fn @aggregate
    // OSAKA-NOT: extcodesize
    // OSAKA: call
    // OSAKA: returndatasize
    function aggregate() external returns (uint256) {
        return NoCodeAggregateTarget(address(0)).pair()[0];
    }

    // A call without return data always needs the check.
    // HOMESTEAD-LABEL: fn @directNoReturn
    // HOMESTEAD: extcodesize
    // HOMESTEAD: call
    // OSAKA-LABEL: fn @directNoReturn
    // OSAKA: extcodesize
    // OSAKA: call
    function directNoReturn() external {
        NoCodeTarget(address(0)).noop();
    }

    // HOMESTEAD-LABEL: fn @pointerNoReturn
    // HOMESTEAD: extcodesize
    // HOMESTEAD: call
    // OSAKA-LABEL: fn @pointerNoReturn
    // OSAKA: extcodesize
    // OSAKA: call
    function pointerNoReturn() external {
        function() external target;
        target();
    }

    function live() external returns (uint256) {
        return NoCodeTarget(address(new NoCodeCallee())).value();
    }

    function liveAggregate() external returns (uint256, uint256) {
        uint256[2] memory values =
            NoCodeAggregateTarget(address(new NoCodeCallee())).pair();
        return (values[0], values[1]);
    }
}
