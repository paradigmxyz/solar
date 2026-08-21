//@ compile-flags: -Ogas
//@ run-call: consistent => true
//@ run-call: emptyAccount => true

// `addr.codehash`, `addr.code`, and `addr.code.length` previously fell
// through builtin member lowering into a struct-field load of the address.
contract AddressCodeMembers {
    function consistent() external view returns (bool) {
        address self = address(this);
        return self.codehash == keccak256(self.code) && self.code.length > 0;
    }

    function emptyAccount() external view returns (bool) {
        address none = address(0xdEaD);
        return none.code.length == 0 && none.codehash == bytes32(0);
    }
}
