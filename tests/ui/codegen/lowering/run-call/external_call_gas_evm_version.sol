//@ revisions: homestead tangerineWhistle
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD
//@[tangerineWhistle] compile-flags: -O none --evm-version tangerineWhistle -Zdump=mir
//@[tangerineWhistle] filecheck: --check-prefix=TANGERINE
//@[homestead,tangerineWhistle] run-call: CallGasCalls::noReturn => 1
//@[homestead,tangerineWhistle] run-call: CallGasCalls::withReturn => 42
//@[homestead,tangerineWhistle] run-call: CallGasCalls::withValue => 7
//@[homestead,tangerineWhistle] run-call: CallGasCalls::twoReturns => 3
//@[homestead,tangerineWhistle] run-call: CallGasCalls::bare => 1
//@[homestead,tangerineWhistle] run-call: CallGasCalls::sendZero => 1

interface CallGasTarget {
    function noop() external;
    function value() external returns (uint256);
    function paid() external payable returns (uint256);
    function pair() external returns (uint256, uint256);
}

contract CallGasCallee {
    function noop() external {}

    function value() external pure returns (uint256) {
        return 42;
    }

    function paid() external payable returns (uint256) {
        return 7;
    }

    function pair() external pure returns (uint256, uint256) {
        return (1, 2);
    }

    receive() external payable {}
}

contract CallGasCalls {
    // Before EIP-150 a gas argument above the gas left aborts the call, so the call withholds
    // its own base cost instead of forwarding `gas()`.
    // HOMESTEAD-LABEL: fn @noReturn
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]],
    // TANGERINE-LABEL: fn @noReturn
    // TANGERINE: [[GAS:v[0-9]+]] = gas
    // TANGERINE: call [[GAS]],
    function noReturn() external returns (uint256) {
        CallGasTarget(address(new CallGasCallee())).noop();
        return 1;
    }

    // HOMESTEAD-LABEL: fn @withReturn
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]],
    // TANGERINE-LABEL: fn @withReturn
    // TANGERINE: [[GAS:v[0-9]+]] = gas
    // TANGERINE: call [[GAS]],
    function withReturn() external returns (uint256) {
        return CallGasTarget(address(new CallGasCallee())).value();
    }

    // A call with a `value` option also withholds the value-transfer cost, 50 + 9000.
    // HOMESTEAD-LABEL: fn @withValue
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 0x235a
    // HOMESTEAD: call [[FWD]],
    // TANGERINE-LABEL: fn @withValue
    // TANGERINE: [[GAS:v[0-9]+]] = gas
    // TANGERINE: call [[GAS]],
    function withValue() external returns (uint256) {
        return CallGasTarget(address(new CallGasCallee())).paid{value: 0}();
    }

    // The output area of a multi-word return gets its own buffer, whose last word is written
    // before the gas is read so that the call's memory expansion is not charged against what the
    // call withholds.
    // HOMESTEAD-LABEL: fn @twoReturns
    // HOMESTEAD: [[BUFFER:v[0-9]+]] = alloc raw, exact, uninitialized, infallible, 64
    // HOMESTEAD: [[LAST:v[0-9]+]] = add [[BUFFER]], 32
    // HOMESTEAD: mstore [[LAST]], 0
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]], {{v[0-9]+}}, 0, {{v[0-9]+}}, {{v[0-9]+}}, [[BUFFER]], 64
    // TANGERINE-LABEL: fn @twoReturns
    // TANGERINE: [[GAS:v[0-9]+]] = gas
    // TANGERINE: [[INPUT:v[0-9]+]] = slice_ptr
    // TANGERINE: call [[GAS]], {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 64
    function twoReturns() external returns (uint256) {
        (uint256 first, uint256 second) =
            CallGasTarget(address(new CallGasCallee())).pair();
        return first + second;
    }

    // A bare call has no `extcodesize` guard, so it also withholds the account-creation cost,
    // 50 + 25000.
    // HOMESTEAD-LABEL: fn @bare
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 0x61da
    // HOMESTEAD: call [[FWD]],
    // TANGERINE-LABEL: fn @bare
    // TANGERINE: [[GAS:v[0-9]+]] = gas
    // TANGERINE: call [[GAS]],
    function bare() external returns (uint256) {
        (bool ok,) = address(new CallGasCallee()).call(abi.encodeWithSignature("noop()"));
        return ok ? 1 : 0;
    }

    // `send` and `transfer` pass a fixed stipend at every version, so they need no reserve.
    // HOMESTEAD-LABEL: fn @sendZero
    // HOMESTEAD-NOT: = gas
    // HOMESTEAD: select {{v[0-9]+}}, 0x8fc, 0
    // TANGERINE-LABEL: fn @sendZero
    // TANGERINE-NOT: = gas
    // TANGERINE: select {{v[0-9]+}}, 0x8fc, 0
    function sendZero() external returns (uint256) {
        return payable(address(new CallGasCallee())).send(0) ? 1 : 0;
    }
}
