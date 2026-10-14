//@ codegen-matrix: standard
//@ run-call: Child::selectors => 0x26121ff0, 0x26121ff0
//@ run-call: Child::precompile => 0x0000000000000000000000000000000000000000
//@ run-call: Child::decoded => 7
//@ run-call: Child::codeHash => true
//@ run-call: Child::widened => 7

// A constant whose value constant evaluation does not compute calls a helper that runs its
// initializer at every use, as solc does. The values come from solc.
contract D {}

contract Base {
    function f() public pure virtual {}
}

contract Child is Base {
    bytes4 constant THIS_SELECTOR = this.f.selector;
    bytes4 constant SUPER_SELECTOR = super.f.selector;
    address constant RECOVERED = ecrecover(bytes32(0), 27, bytes32(0), bytes32(0));
    uint8 constant DECODED = abi.decode(abi.encode(uint8(7)), (uint8));
    // The initializer converts to the declared type.
    uint256 constant WIDENED = DECODED;
    bytes32 constant CODE_HASH = keccak256(type(D).creationCode);

    function f() public pure override {}

    function selectors() external pure returns (bytes4, bytes4) {
        return (THIS_SELECTOR, SUPER_SELECTOR);
    }

    function precompile() external pure returns (address) {
        return RECOVERED;
    }

    function decoded() external pure returns (uint8) {
        return DECODED;
    }

    function codeHash() external pure returns (bool) {
        return CODE_HASH == keccak256(type(D).creationCode);
    }

    function widened() external pure returns (uint256) {
        return WIDENED;
    }
}
