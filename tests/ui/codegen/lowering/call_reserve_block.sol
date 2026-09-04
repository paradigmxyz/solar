//@ revisions: gas size tangerineWhistle
//@[gas] compile-flags: -O gas --evm-version homestead -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefix=RESERVE --implicit-check-not=gas
//@[size] compile-flags: -O size --evm-version homestead -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefix=RESERVE --implicit-check-not=gas
//@[tangerineWhistle] compile-flags: -O gas --evm-version tangerineWhistle -Zdump=evm-ir-runtime
//@[tangerineWhistle] filecheck: --check-prefix=TANGERINE

// Before EIP-150 a `CALL` asking for more gas than is left throws, so a call that forwards the
// gas left withholds its own cost plus solc's 10-gas margin, `sub(gas(), 50)`. The margin only
// pays for the `SUB`, so the reserve is only correct while the `GAS` read and the call stay in
// one block: a `JUMP` (8) and a `JUMPDEST` (1) in between overrun it and the call throws. A
// forwarded-gas call next to a `{gas: ...}` call of the same shape is exactly the pair of equal
// call tails the backend wants to share, and merging them used to cut the block between the
// `SUB` and the `CALL`.

interface ReserveTarget {
    function value() external returns (uint256);
}

contract ReserveCallee {
    function value() external pure returns (uint256) {
        return 42;
    }
}

contract ReserveBlock {
    ReserveTarget internal callee;

    constructor() {
        callee = ReserveTarget(address(new ReserveCallee()));
    }

    // `keep_with_next` pins the reserve to the call, so nothing can turn either boundary into a
    // block boundary. The implicit check-not covers every other `gas` in the runtime object, so a
    // pass that splits this sequence fails the test. From EIP-150 on the gas is capped instead of
    // rejected, the reserve is gone, and the plain `gas` read carries no metadata.
    // RESERVE: gas !meta(keep_with_next)
    // RESERVE-NEXT: sub !meta(keep_with_next)
    // RESERVE-NEXT: call
    // TANGERINE-NOT: keep_with_next
    function fixedGas() external returns (uint256) {
        return callee.value{gas: 50000}();
    }

    function pointer() external returns (uint256) {
        function() external returns (uint256) f = callee.value;
        return f();
    }
}
