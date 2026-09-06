//@ revisions: homestead homesteadGas homesteadSize tangerineWhistle
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD
//@[homesteadGas] compile-flags: -O gas --evm-version homestead
//@[homesteadSize] compile-flags: -O size --evm-version homestead
//@[tangerineWhistle] compile-flags: -O none --evm-version tangerineWhistle -Zdump=mir
//@[tangerineWhistle] filecheck: --check-prefix=TANGERINE
//@ run-call: WideCalls::plainCall => 9
//@ run-call: WideCalls::tryCall => 9
//@ run-call: WideCalls::stringCall => 9

// A pre-EIP-150 `CALL` is charged its own memory expansion out of the gas left before the
// forwarded gas is compared against the remainder, and `sub(gas(), 50)` leaves seven gas for it,
// two words at most. A six-word output area behind a dynamically encoded argument reaches far
// enough past the argument's last word to overrun that, so the word above the area is touched
// before the arguments are encoded, where its address does not depend on what they encode to.

interface Wide {
    function six(bytes memory b)
        external
        returns (uint256, uint256, uint256, uint256, uint256, uint256);
    function sixString(string memory s)
        external
        returns (uint256, uint256, uint256, uint256, uint256, uint256);
}

contract WideCallee {
    function six(bytes memory b)
        external
        pure
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        return (b.length + 4, 1, 2, 3, 4, 5);
    }

    function sixString(string memory s)
        external
        pure
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        return (bytes(s).length + 4, 1, 2, 3, 4, 5);
    }
}

contract WideCalls {
    // The touch is `mstore(add(fmp(), 192), 0)`, six words above the area's own start, and it
    // precedes the encoding it would otherwise write over.
    // HOMESTEAD-LABEL: fn @plainCall
    // HOMESTEAD: create
    // HOMESTEAD: [[AREA:v[0-9]+]] = fmp
    // HOMESTEAD: [[ABOVE:v[0-9]+]] = add [[AREA]], 192
    // HOMESTEAD: mstore [[ABOVE]], 0
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]], {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 192
    // From EIP-150 on the forwarded gas is capped instead of rejected, so neither the reserve nor
    // the touch is needed.
    // TANGERINE-LABEL: fn @plainCall
    // TANGERINE-NOT: = fmp
    // TANGERINE: [[GAS:v[0-9]+]] = gas
    // TANGERINE: call [[GAS]],
    function plainCall() external returns (uint256) {
        (uint256 a,,,,, uint256 f) = Wide(address(new WideCallee())).six("");
        return a + f;
    }

    // A `try` around the same call touches the area the same way; its catch clause cannot absorb
    // an out-of-gas.
    // HOMESTEAD-LABEL: fn @tryCall
    // HOMESTEAD: create
    // HOMESTEAD: [[AREA:v[0-9]+]] = fmp
    // HOMESTEAD: [[ABOVE:v[0-9]+]] = add [[AREA]], 192
    // HOMESTEAD: mstore [[ABOVE]], 0
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]], {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 192
    function tryCall() external returns (uint256 r) {
        try Wide(address(new WideCallee())).six("") returns (
            uint256 a, uint256, uint256, uint256, uint256, uint256 f
        ) {
            r = a + f;
        } catch {
            r = 7;
        }
    }

    // HOMESTEAD-LABEL: fn @stringCall
    // HOMESTEAD: create
    // HOMESTEAD: [[AREA:v[0-9]+]] = fmp
    // HOMESTEAD: [[ABOVE:v[0-9]+]] = add [[AREA]], 192
    // HOMESTEAD: mstore [[ABOVE]], 0
    function stringCall() external returns (uint256) {
        (uint256 a,,,,, uint256 f) = Wide(address(new WideCallee())).sixString("");
        return a + f;
    }
}
