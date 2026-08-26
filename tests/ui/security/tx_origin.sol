//@ compile-flags: -Zcodegen -Zsecurity --emit=bin-runtime

contract TxOrigin {
    address owner;

    function withdraw() external view returns (bool) {
        return tx.origin == owner; //~ WARN: use of `tx.origin`
    }

    function transferTo(address to) external returns (bool) {
        if (tx.origin != owner) { //~ WARN: use of `tx.origin`
            return false;
        }
        owner = to;
        return true;
    }

    // Uses `msg.sender`, so no finding should be reported here.
    function authed() external view returns (bool) {
        return msg.sender == owner;
    }
}
