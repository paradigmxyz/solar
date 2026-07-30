//@compile-flags: -Zcodegen --emit=bin-runtime
// ported-from: contracts/test/Reenterer.sol

// A storage `bytes` used directly as low-level call data. A call reads its
// input from memory, so the stored value's short/long form is decoded into
// memory first — the same materialization every other storage-bytes read
// uses. Verified against solc for both forms.
contract LowLevelCallStorageBytes {
    bytes public callData;
    address public target;

    function prepare(address t, bytes calldata d) external {
        target = t;
        callData = d;
    }

    function fire() external returns (bool ok, bytes memory ret) {
        (ok, ret) = target.call(callData);
    }
}
