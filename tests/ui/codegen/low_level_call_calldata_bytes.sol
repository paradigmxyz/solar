//@compile-flags: -Zcodegen --emit=evm-ir-runtime

// A low-level call may take a `bytes calldata` value as its call data, e.g. a
// proxy fallback `impl.delegatecall(data)` with `bytes calldata data` (aave-v3
// BaseImmutableAdminUpgradeabilityProxy, pulled in by ConfiguratorLogic). The
// calldata bytes are copied into memory (a call reads its input from memory)
// and used as the call's `[offset, size]`. Runtime behavior — success flag for
// good/reverting/unknown selectors, and delegatecall — is verified equal to
// solc 0.8.30 separately.

contract C {
    function callFwd(address t, bytes calldata data) external returns (bool) {
        (bool ok, ) = t.call(data);
        return ok;
    }

    function delegateFwd(address t, bytes calldata data) external returns (bool) {
        (bool ok, ) = t.delegatecall(data);
        return ok;
    }
}
