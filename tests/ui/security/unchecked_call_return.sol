//@ compile-flags: -Zcodegen -Zsecurity --emit=bin-runtime

// A low-level call returns a success flag; ignoring it lets a failed call pass
// as success. The analysis reports a call whose result is never used. Checking
// the result — or `.transfer`, whose compiler-generated revert consumes it — is
// not flagged (the false-positive guard). Targets are read from storage so this
// isolates the unchecked-return finding from the arbitrary-send finding.

contract UncheckedCall {
    address stored;

    // The success flag is dropped: unchecked.
    function ignore() external {
        stored.call(""); //~ WARN: return value of low-level call is ignored
    }

    // `.send` returns a bool that is dropped: unchecked.
    function sendDrop() external {
        payable(stored).send(1); //~ WARN: return value of low-level call is ignored
    }

    // The success flag is checked: no finding.
    function checked() external returns (bool ok) {
        (ok, ) = stored.call("");
        require(ok);
    }

    // `.transfer` reverts on failure via a compiler-generated check that
    // consumes the result: no finding.
    function transferChecked() external {
        payable(stored).transfer(1);
    }
}
