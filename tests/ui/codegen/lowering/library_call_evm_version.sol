//@ revisions: homestead byzantium
//@[homestead] compile-flags: -O none --evm-version homestead --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD --implicit-check-not=returndatasize
//@[byzantium] compile-flags: -O none --evm-version byzantium --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@[byzantium] filecheck: --check-prefix=BYZANTIUM

library Lib {
    function dbl(uint256 x) external pure returns (uint256) {
        return 2 * x;
    }

    function pair(uint256 x) external pure returns (uint256, uint256) {
        return (x, x + 1);
    }

    function arr(uint256 x) external pure returns (uint256[2] memory) {
        return [x, x];
    }

    function noret(uint256) external pure {}
}

contract C {
    // Before Byzantium a linked-library call takes its return values out of the delegatecall's
    // static output area, as solc's `delegatecall(..., out, 32)` does; from Byzantium on they come
    // out of the return data, and the length check there subsumes the code check.
    // HOMESTEAD-LABEL: fn @one
    // HOMESTEAD: extcodesize
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: delegatecall [[FWD]], {{.*}}, 0, 32
    // HOMESTEAD: mload
    // BYZANTIUM-LABEL: fn @one
    // BYZANTIUM-NOT: extcodesize
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function one(uint256 x) external pure returns (uint256) {
        return Lib.dbl(x);
    }

    // Two words get an output area of their own: the words the call writes above the arguments are
    // untouched memory, whose expansion a pre-EIP-150 call charges before checking the gas it
    // forwards.
    // HOMESTEAD-LABEL: fn @two
    // HOMESTEAD: [[BUF:v[0-9]+]] = alloc raw, exact, uninitialized, infallible, 64
    // HOMESTEAD: [[LAST:v[0-9]+]] = add [[BUF]], 32
    // HOMESTEAD: mstore [[LAST]], 0
    // HOMESTEAD: delegatecall {{.*}}, [[BUF]], 64
    // HOMESTEAD: mload
    // HOMESTEAD: mload
    // BYZANTIUM-LABEL: fn @two
    // BYZANTIUM-NOT: extcodesize
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function two(uint256 x) external pure returns (uint256 a, uint256 b) {
        (a, b) = Lib.pair(x);
    }

    // A statically encoded aggregate is decoded out of the output area itself.
    // HOMESTEAD-LABEL: fn @aggregate
    // HOMESTEAD: [[BUF:v[0-9]+]] = alloc raw, exact, uninitialized, infallible, 64
    // HOMESTEAD: [[LAST:v[0-9]+]] = add [[BUF]], 32
    // HOMESTEAD: mstore [[LAST]], 0
    // HOMESTEAD: delegatecall {{.*}}, [[BUF]], 64
    // HOMESTEAD: abi_decode {{.*}}, [[BUF]]
    // BYZANTIUM-LABEL: fn @aggregate
    // BYZANTIUM-NOT: extcodesize
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function aggregate(uint256 x) external pure returns (uint256[2] memory) {
        return Lib.arr(x);
    }

    // A call with no return values declares no output area at either version.
    // HOMESTEAD-LABEL: fn @none
    // HOMESTEAD: extcodesize
    // HOMESTEAD: delegatecall {{.*}}, 0, 0
    // BYZANTIUM-LABEL: fn @none
    // BYZANTIUM: extcodesize
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    function none(uint256 x) external pure {
        Lib.noret(x);
    }
}
