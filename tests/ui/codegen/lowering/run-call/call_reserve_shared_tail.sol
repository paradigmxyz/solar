//@ revisions: homestead homesteadGas homesteadSize tangerineWhistleGas
//@[homestead] compile-flags: -O none --evm-version homestead
//@[homesteadGas] compile-flags: -O gas --evm-version homestead -Zdump=evm-ir-runtime
//@[homesteadGas] filecheck: --check-prefix=RESERVE --implicit-check-not={{^[ ]+gas}}
//@[homesteadSize] compile-flags: -O size --evm-version homestead -Zdump=evm-ir-runtime
//@[homesteadSize] filecheck: --check-prefix=RESERVE --implicit-check-not={{^[ ]+gas}}
//@[tangerineWhistleGas] compile-flags: -O gas --evm-version tangerineWhistle
//@ run-call: ReserveCalls::fixedGas => 42
//@ run-call: ReserveCalls::two => 3
//@ run-call: ReserveCalls::aggregate => 7
//@ run-call: ReserveCalls::noReturn => 1
//@ run-call: ReserveCalls::pointer => 42
//@ run-call: ReserveCalls::twoFixed => 3

// Before EIP-150 a `CALL` asking for more gas than is left throws, so a call that forwards the
// gas left withholds its own cost plus solc's 10-gas margin (`sub(gas(), 50)`). That margin only
// covers the `SUB`, so the reserve stays correct only while the `GAS` read and the call are in
// one block: a shared call tail puts a `JUMP` (8) and a `JUMPDEST` (1) in between and the call
// throws. Mixing forwarded-gas calls with `{gas: ...}` calls of the same shape gives the backend
// the equal tails it wants to merge, and every call below runs on a homestead EVM.

interface ReserveTarget {
    function value() external returns (uint256);
    function pair() external returns (uint256, uint256);
    function agg() external returns (uint256[2] memory);
    function noop() external;
}

contract ReserveCallee {
    function value() external pure returns (uint256) {
        return 42;
    }

    function pair() external pure returns (uint256, uint256) {
        return (1, 2);
    }

    function agg() external pure returns (uint256[2] memory r) {
        r[0] = 3;
        r[1] = 4;
    }

    function noop() external {}
}

contract ReserveCalls {
    ReserveTarget internal callee;

    constructor() {
        callee = ReserveTarget(address(new ReserveCallee()));
    }

    // Every `gas` in the runtime object is a reserve read, and `keep_with_next` pins each one to
    // the `sub` and the `call` that follow it in the same block. The implicit check-not covers
    // every other `gas` line, so a pass that splits one of these sequences fails the test.
    // RESERVE: gas !metadata(keep_with_next)
    // RESERVE-NEXT: sub !metadata(keep_with_next)
    // RESERVE-NEXT: call
    // RESERVE: gas !metadata(keep_with_next)
    // RESERVE-NEXT: sub !metadata(keep_with_next)
    // RESERVE-NEXT: call
    // RESERVE: gas !metadata(keep_with_next)
    // RESERVE-NEXT: sub !metadata(keep_with_next)
    // RESERVE-NEXT: call
    // RESERVE: gas !metadata(keep_with_next)
    // RESERVE-NEXT: sub !metadata(keep_with_next)
    // RESERVE-NEXT: call
    function fixedGas() external returns (uint256) {
        return callee.value{gas: 50000}();
    }

    function two() external returns (uint256) {
        (uint256 x, uint256 y) = callee.pair();
        return x + y;
    }

    function aggregate() external returns (uint256) {
        uint256[2] memory v = callee.agg();
        return v[0] + v[1];
    }

    function noReturn() external returns (uint256) {
        callee.noop();
        return 1;
    }

    function pointer() external returns (uint256) {
        function() external returns (uint256) f = callee.value;
        return f();
    }

    function twoFixed() external returns (uint256) {
        (uint256 x, uint256 y) = callee.pair{gas: 50000}();
        return x + y;
    }
}
