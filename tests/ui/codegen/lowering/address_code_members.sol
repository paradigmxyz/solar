//@ run-call: codeFromWord 0x1000000000000000000000000000000000000dead => 0x
//@ compile-flags: -Ogas
//@ run-call: consistent => true
//@ run-call: emptyAccount => true

// `addr.codehash`, `addr.code`, and `addr.code.length` previously fell
// through builtin member lowering into a struct-field load of the address.
contract AddressCodeMembers {
    function codeFromWord(uint256 word) external view returns (bytes memory) {
        return address(uint160(word)).code;
    }

    function consistent() external view returns (bool) {
        address self = address(this);
        return self.codehash == keccak256(self.code) && self.code.length > 0;
    }

    function emptyAccount() external view returns (bool) {
        address none = address(0xdEaD);
        return none.code.length == 0 && none.codehash == bytes32(0);
    }
}
