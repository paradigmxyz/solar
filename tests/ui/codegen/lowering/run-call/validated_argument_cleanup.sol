//@ codegen-matrix: standard
//@ run-call: clean 0x0000000000000000000000000000000000000001, 255, 0x12345678 => 1, 255, 0x12345678
//@ run-call: clean 0xffffffffffffffffffffffffffffffffffffffff, 0, 0xffffffff => 1461501637330902918203684832716283019655932542975, 0, 0xffffffff
//@ run-call: internalDirty 1461501637330902918203684832716283019655932542977 => 1
//@ run-call: maskedByte 0x0000000000000000000000000000000000000101 => 1
//@ run-call: choice 2 => 2
//@ run-call: signed -1 => -1
//@ run-call-fail: 0x73889fbb000000000000000000000001000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000ff1234567800000000000000000000000000000000000000000000000000000000 => 0x
//@ run-call-fail: 0x73889fbb000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000001001234567800000000000000000000000000000000000000000000000000000000 => 0x
//@ run-call-fail: 0x73889fbb000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000ff1234567800000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call-fail: 0x5cbeb69a0000000000000000000000000000000000000000000000000000000000000003 => 0x
//@ run-call-fail: 0xa222272400000000000000000000000000000000000000000000000000000000000000ff => 0x
//@ run-call-fail: 0x73889fbb => 0x
//@ run-call: maskedTag 0x12345678 => 0x1234567800000000000000000000000000000000000000000000000000000000
//@ run-call: choiceMask 2 => 2
//@ run-call-fail: 0x179f850e1234567800000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call-fail: 0xebf04b360000000000000000000000000000000000000000000000000000000000000003 => 0x

// Validated words need no second canonical mask. Raw internal calls and
// narrower masks retain their cleanup, and malformed ABI words still revert.
contract ValidatedArgumentCleanup {
    enum Choice { First, Second, Third }

    function clean(address account, uint8 small, bytes4 tag)
        external pure returns (uint256, uint256, bytes4)
    {
        return (uint256(uint160(account)), uint256(small), tag);
    }

    function publicAddress(address account) public pure returns (uint256) {
        return uint256(uint160(account));
    }

    function internalDirty(uint256 raw) external pure returns (uint256) {
        address account;
        assembly { account := raw }
        return publicAddress(account);
    }

    function maskedByte(address account) external pure returns (uint256) {
        return uint256(uint160(account)) & 255;
    }

    function choice(Choice value) external pure returns (uint256) {
        return uint256(value);
    }

    function signed(int8 value) external pure returns (int256) {
        return int256(value);
    }
    function maskedTag(bytes4 value) external pure returns (bytes32 result) {
        assembly {
            result := and(value, 0xffffffff00000000000000000000000000000000000000000000000000000000)
        }
    }

    function choiceMask(Choice value) external pure returns (uint256 result) {
        assembly { result := and(value, 0xff) }
    }
}
