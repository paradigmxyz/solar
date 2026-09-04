//@ revisions: homestead homesteadGas homesteadSize tangerineWhistle
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD
// The reserve only covers the `SUB` and the `CALL`, and the output-area touch is a store the call
// itself overwrites, so both survive only as long as no pass moves work between the `GAS` and the
// call or eliminates the store. The optimized revisions run the same calls on a homestead EVM.
//@[homesteadGas] compile-flags: -O gas --evm-version homestead
//@[homesteadSize] compile-flags: -O size --evm-version homestead
//@[tangerineWhistle] compile-flags: -O none --evm-version tangerineWhistle -Zdump=mir
//@[tangerineWhistle] filecheck: --check-prefix=TANGERINE
//@ run-call: CallGasCalls::noReturn => 1
//@ run-call: CallGasCalls::withReturn => 42
//@ run-call: CallGasCalls::withValue => 7
//@ run-call: CallGasCalls::twoReturns => 3
//@ run-call: CallGasCalls::eightReturns => 36
//@ run-call: CallGasCalls::aggregate => 33
//@ run-call: CallGasCalls::bare => 1
//@ run-call: CallGasCalls::sendZero => 1

interface CallGasTarget {
    function noop() external;
    function value() external returns (uint256);
    function paid() external payable returns (uint256);
    function pair() external returns (uint256, uint256);
    function eight()
        external
        returns (uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256);
    function agg() external returns (uint256[2] memory);
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

    function eight()
        external
        pure
        returns (uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256)
    {
        return (1, 2, 3, 4, 5, 6, 7, 8);
    }

    function agg() external pure returns (uint256[2] memory r) {
        r[0] = 11;
        r[1] = 22;
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

    // The output area of a multi-word return overlays the input area, and the word above it is
    // written before the arguments are encoded and the gas is read, so that the call's memory
    // expansion is not charged against what the call withholds.
    // HOMESTEAD-LABEL: fn @twoReturns
    // HOMESTEAD: create
    // HOMESTEAD: [[AREA:v[0-9]+]] = fmp
    // HOMESTEAD: [[ABOVE:v[0-9]+]] = add [[AREA]], 64
    // HOMESTEAD: mstore [[ABOVE]], 0
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]], {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 64
    // TANGERINE-LABEL: fn @twoReturns
    // TANGERINE: [[GAS:v[0-9]+]] = gas
    // TANGERINE: [[INPUT:v[0-9]+]] = slice_ptr
    // TANGERINE: call [[GAS]], {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 64
    function twoReturns() external returns (uint256) {
        (uint256 first, uint256 second) =
            CallGasTarget(address(new CallGasCallee())).pair();
        return first + second;
    }

    // A wider output area is touched one word above its own size, whatever the input encodes to.
    // HOMESTEAD-LABEL: fn @eightReturns
    // HOMESTEAD: create
    // HOMESTEAD: [[AREA:v[0-9]+]] = fmp
    // HOMESTEAD: [[ABOVE:v[0-9]+]] = add [[AREA]], 256
    // HOMESTEAD: mstore [[ABOVE]], 0
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]], {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 256
    function eightReturns() external returns (uint256) {
        (
            uint256 a1,
            uint256 a2,
            uint256 a3,
            uint256 a4,
            uint256 a5,
            uint256 a6,
            uint256 a7,
            uint256 a8
        ) = CallGasTarget(address(new CallGasCallee())).eight();
        return a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8;
    }

    // A static aggregate return is copied out of the overlaid area into a buffer of its own, and
    // the word above the area is touched too.
    // HOMESTEAD-LABEL: fn @aggregate
    // HOMESTEAD: create
    // HOMESTEAD: [[AREA:v[0-9]+]] = fmp
    // HOMESTEAD: [[ABOVE:v[0-9]+]] = add [[AREA]], 64
    // HOMESTEAD: mstore [[ABOVE]], 0
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]],
    function aggregate() external returns (uint256) {
        uint256[2] memory r = CallGasTarget(address(new CallGasCallee())).agg();
        return r[0] + r[1];
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
