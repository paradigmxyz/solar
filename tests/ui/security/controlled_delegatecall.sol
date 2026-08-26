//@ compile-flags: -Zsecurity --emit=bin-runtime --allow=2264

// The security analysis traces each `delegatecall` target backward through the
// MIR SSA graph. A target derived from an external argument or calldata is an
// attacker-controlled delegatecall (full-takeover risk) and is reported; a
// target held in storage — the ordinary upgradeable-proxy pattern — is not (the
// false-positive guard).

contract Delegate {
    address stored;

    // `impl` comes straight from calldata: controlled.
    function forward(address impl, bytes calldata data) external returns (bool ok) {
        (ok, ) = impl.delegatecall(data); //~ WARN: `delegatecall` to an attacker-controlled address
    }

    // The masked argument is still attacker-derived: controlled.
    function forwardMasked(address impl, bytes calldata data) external returns (bool ok) {
        address target = address(uint160(uint256(uint160(impl)) & type(uint160).max));
        (ok, ) = target.delegatecall(data); //~ WARN: `delegatecall` to an attacker-controlled address
    }

    // The target is read from storage, not attacker input: no finding.
    function forwardStored(bytes calldata data) external returns (bool ok) {
        (ok, ) = stored.delegatecall(data);
    }
}
