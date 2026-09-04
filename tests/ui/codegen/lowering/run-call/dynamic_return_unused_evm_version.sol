//@ revisions: homestead homesteadGas homesteadSize byzantium osaka
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD --implicit-check-not=returndatasize --implicit-check-not=returndatacopy
//@[homesteadGas] compile-flags: -O gas --evm-version homestead
//@[homesteadSize] compile-flags: -O size --evm-version homestead
//@[byzantium] compile-flags: -O none --evm-version byzantium
//@[osaka] compile-flags: -O none --evm-version osaka -Zdump=mir
//@[osaka] filecheck: --check-prefix=OSAKA
//@ run-call: DynamicReturnUnused::live => 1
//@ run-call: DynamicReturnUnused::liveString => 1
//@ run-call: DynamicReturnUnused::liveTry => 1
//@ run-call: DynamicReturnUnused::liveMixed => 11
//@ run-call: DynamicReturnUnused::liveTuple => 1
//@ run-call: DynamicReturnUnused::livePointer => 1
//@ run-call: DynamicReturnUnused::liveLoop => 3
//@ run-call-fail: DynamicReturnUnused::noCode => 0x
//@ run-call-fail: DynamicReturnUnused::noCodeTry => 0x

interface DynamicTarget {
    function dynBytes() external returns (bytes memory);
    function dynString() external returns (string memory);
    function mixed() external returns (uint256, bytes memory);
}

contract DynamicCallee {
    function dynBytes() external pure returns (bytes memory) {
        return "0123456789abcdef0123456789abcdef01";
    }

    function dynString() external pure returns (string memory) {
        return "hello";
    }

    function mixed() external pure returns (uint256, bytes memory) {
        return (11, "abc");
    }
}

contract DynamicReturnUnused {
    // Before Byzantium a dynamically encoded return value is inaccessible: the call reserves a
    // word of output area for it, nothing decodes it, and the `extcodesize` guard and the
    // pre-EIP-150 gas reserve still apply.
    // HOMESTEAD-LABEL: fn @live()
    // HOMESTEAD: create
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: extcodesize
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: [[OK:v[0-9]+]] = call [[FWD]], {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 32
    // HOMESTEAD: jumpi [[OK]]
    // OSAKA-LABEL: fn @live()
    // OSAKA: [[OK:v[0-9]+]] = call {{v[0-9]+}}, {{v[0-9]+}}, 0, {{v[0-9]+}}, {{v[0-9]+}}, 0, 0
    // OSAKA: jumpi [[OK]]
    function live() external returns (uint256) {
        DynamicTarget(address(new DynamicCallee())).dynBytes();
        return 1;
    }

    function liveString() external returns (uint256) {
        DynamicTarget(address(new DynamicCallee())).dynString();
        return 1;
    }

    // A bare `catch` around an inaccessible value needs no return data either.
    // HOMESTEAD-LABEL: fn @liveTry()
    // HOMESTEAD: create
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: extcodesize
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: [[OK:v[0-9]+]] = call [[FWD]], {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 32
    // HOMESTEAD: jumpi [[OK]]
    function liveTry() external returns (uint256 r) {
        try DynamicTarget(address(new DynamicCallee())).dynBytes() {
            r = 1;
        } catch {
            r = 2;
        }
    }

    // The static components of a mixed return stay accessible, so the output area covers both
    // words and the first one is read back from the input area it overlays.
    // HOMESTEAD-LABEL: fn @liveMixed()
    // HOMESTEAD: create
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: [[LAST:v[0-9]+]] = add [[INPUT]], 32
    // HOMESTEAD: mstore [[LAST]], 0
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]], {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 64
    // HOMESTEAD: [[FIRST:v[0-9]+]] = add [[INPUT]], 0
    // HOMESTEAD: mload [[FIRST]]
    function liveMixed() external returns (uint256 r) {
        (uint256 v, ) = DynamicTarget(address(new DynamicCallee())).mixed();
        r = v;
    }

    // A tuple expression statement discards every component, so both calls are lowered like a
    // discarded one.
    function liveTuple() external returns (uint256) {
        DynamicTarget target = DynamicTarget(address(new DynamicCallee()));
        (target.dynBytes(), target.dynString());
        return 1;
    }

    function livePointer() external returns (uint256) {
        DynamicTarget target = DynamicTarget(address(new DynamicCallee()));
        function() external returns (bytes memory) f = target.dynBytes;
        f();
        return 1;
    }

    function liveLoop() external returns (uint256 r) {
        DynamicTarget target = DynamicTarget(address(new DynamicCallee()));
        for (uint256 i = 0; i < 3; i++) {
            target.dynBytes();
            r++;
        }
    }

    // A code-less callee reverts: the guard is what a pre-Byzantium call has in place of a
    // return-data check, and from Byzantium on the empty return data fails to decode.
    // HOMESTEAD-LABEL: fn @noCode()
    // HOMESTEAD: extcodesize
    function noCode() external returns (uint256) {
        DynamicTarget(address(0)).dynBytes();
        return 1;
    }

    // HOMESTEAD-LABEL: fn @noCodeTry()
    // HOMESTEAD: extcodesize
    function noCodeTry() external returns (uint256 r) {
        try DynamicTarget(address(0)).dynBytes() {
            r = 1;
        } catch {
            r = 2;
        }
    }
}
