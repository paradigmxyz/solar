//@ revisions: homestead homesteadGas homesteadSize tangerineWhistle spuriousDragon
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD
//@[homesteadGas] compile-flags: -O gas --evm-version homestead
//@[homesteadSize] compile-flags: -O size --evm-version homestead
//@[tangerineWhistle] compile-flags: -O none --evm-version tangerineWhistle
//@[spuriousDragon] compile-flags: -O none --evm-version spuriousDragon
//@ run-call: ShortReturns::emptySingle => 0x3fa4f24500000000000000000000000000000000000000000000000000000000
//@ run-call: ShortReturns::emptySingleArg => 0x8308677200000000000000000000000000000000000000000000000000000000
//@ run-call: ShortReturns::emptyPair => 0xa8aa1b3100000000000000000000000000000000000000000000000000000000, 0
//@ run-call: ShortReturns::emptyFour => 0xa1fca2b600000000000000000000000000000000000000000000000000000000, 0, 0, 0
//@ run-call: ShortReturns::emptyAgg => [0xf5e34bfa00000000000000000000000000000000000000000000000000000000, 0]
//@ run-call: ShortReturns::emptyAggArg => [0xf5a636f000000000000000000000000000000000000000000000000000000000, 0x0000000100000000000000000000000000000000000000000000000000000000]
//@ run-call: ShortReturns::partialSingle => 0x1122334400000000000000000000000000000000000000000000000000000000
//@ run-call: ShortReturns::partialPair => 0x1122334455667788990011223344556677889900112233445566778899001122, 0
//@ run-call: ShortReturns::partialAgg => [0x1122334455667788990011223344556677889900112233445566778899001122, 0]
//@ run-call: ShortReturns::dirtyFour => 0x1122334400000000000000000000000000000000000000000000000000000000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xdeadbeef, 0x1234

// Before Byzantium a callee that returns fewer bytes than it declares cannot be detected: there
// is no `RETURNDATASIZE` to compare against, so the decoded values are whatever the output area
// holds. solc overlays that area on the call's own input area, so the missing bytes read back as
// the selector and arguments the call left there, and every value below is solc 0.8.36's.

interface Target {
    function value() external returns (uint256);
    function valueArg(uint256 a) external returns (uint256);
    function pair() external returns (uint256, uint256);
    function four() external returns (uint256, uint256, uint256, uint256);
    function agg() external returns (uint256[2] memory);
    function aggArg(uint256 a, uint256 b, uint256 c) external returns (uint256[2] memory);
    function shortWord() external returns (uint256);
    function shortPair() external returns (uint256, uint256);
    function shortAgg() external returns (uint256[2] memory);
    function shortFour() external returns (uint256, uint256, uint256, uint256);
}

// Every call targets this contract itself, whose fallback returns no data at all and whose
// `short*` functions return a truncated word.
contract ShortReturns {
    fallback() external {}

    function shortWord() external pure {
        assembly {
            mstore(0, 0x1122334455667788990011223344556677889900112233445566778899001122)
            return(0, 4)
        }
    }

    function shortPair() external pure {
        assembly {
            mstore(0, 0x1122334455667788990011223344556677889900112233445566778899001122)
            return(0, 36)
        }
    }

    function shortAgg() external pure {
        assembly {
            mstore(0, 0x1122334455667788990011223344556677889900112233445566778899001122)
            return(0, 36)
        }
    }

    function shortFour() external pure {
        assembly {
            mstore(0, 0x1122334455667788990011223344556677889900112233445566778899001122)
            return(0, 4)
        }
    }

    // A single-word output area starts at the input area, so the word the call reads back keeps
    // the selector, and the store to the scratch word below the heap is not part of it.
    // HOMESTEAD-LABEL: fn @emptySingle
    // HOMESTEAD: mstore 0, 42
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: call {{v[0-9]+}}, {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 32
    function emptySingle() external returns (uint256) {
        assembly {
            mstore(0, 42)
        }
        return Target(address(this)).value();
    }

    // The argument stays in the area the output overlays, so its own leading bytes read back.
    function emptySingleArg() external returns (uint256) {
        return Target(address(this)).valueArg(0xdead);
    }

    // HOMESTEAD-LABEL: fn @emptyPair
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: call {{v[0-9]+}}, {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 64
    function emptyPair() external returns (uint256, uint256) {
        assembly {
            mstore(0, 42)
            mstore(32, 43)
        }
        return Target(address(this)).pair();
    }

    // The word above this output area is written before the arguments are encoded and the gas is
    // read, so the call is not charged the expansion out of what it withholds.
    // HOMESTEAD-LABEL: fn @emptyFour
    // HOMESTEAD: [[AREA:v[0-9]+]] = fmp
    // HOMESTEAD: [[ABOVE:v[0-9]+]] = add [[AREA]], 128
    // HOMESTEAD: mstore [[ABOVE]], 0
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: call [[FWD]], {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 128
    function emptyFour() external returns (uint256, uint256, uint256, uint256) {
        return Target(address(this)).four();
    }

    // An aggregate return decodes out of a buffer taken before the arguments, so the copy out of
    // the overlaid area cannot run into memory the decoding allocates above it.
    // HOMESTEAD-LABEL: fn @emptyAgg
    // HOMESTEAD: [[BUFFER:v[0-9]+]] = alloc memorybytes
    // HOMESTEAD: [[DATA:v[0-9]+]] = memory_object_data memorybytes, [[BUFFER]]
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: call {{v[0-9]+}}, {{v[0-9]+}}, 0, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 64
    // HOMESTEAD: mcopy [[DATA]], [[INPUT]], 64
    // HOMESTEAD: abi_decode {{.*}}, [[BUFFER]]
    function emptyAgg() external returns (uint256[2] memory) {
        return Target(address(this)).agg();
    }

    function emptyAggArg() external returns (uint256[2] memory) {
        return Target(address(this)).aggArg(1, 2, 3);
    }

    function partialSingle() external returns (uint256) {
        return Target(address(this)).shortWord();
    }

    function partialPair() external returns (uint256, uint256) {
        return Target(address(this)).shortPair();
    }

    function partialAgg() external returns (uint256[2] memory) {
        return Target(address(this)).shortAgg();
    }

    // The words the callee leaves untouched are read back as whatever memory above the free
    // pointer already held, which is what the touch above the output area preserves: a store
    // inside the area would hand back zeros where solc hands back the assembly's writes.
    // HOMESTEAD-LABEL: fn @dirtyFour
    // HOMESTEAD: [[AREA:v[0-9]+]] = fmp
    // HOMESTEAD: [[ABOVE:v[0-9]+]] = add [[AREA]], 128
    // HOMESTEAD: mstore [[ABOVE]], 0
    // HOMESTEAD: [[INPUT:v[0-9]+]] = slice_ptr
    // HOMESTEAD: call {{.*}}, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 128
    function dirtyFour() external returns (uint256, uint256, uint256, uint256) {
        assembly {
            let p := mload(0x40)
            mstore(add(p, 32), not(0))
            mstore(add(p, 64), 0xdeadbeef)
            mstore(add(p, 96), 0x1234)
        }
        return Target(address(this)).shortFour();
    }
}
