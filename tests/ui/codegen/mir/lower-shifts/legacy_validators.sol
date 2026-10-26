//@ codegen-matrix: standard
//@ compile-flags: --evm-version byzantium
//@ run-call: word8 255 => 255
//@ run-call-fail: 0x8464342e0000000000000000000000000000000000000000000000000000000000000100
//@ run-call: flag true => 1
//@ run-call-fail: 0xa92a4c3b0000000000000000000000000000000000000000000000000000000000000002
//@ run-call: selector 0x11223344 => 0x11223344
//@ run-call-fail: 0x9d3d6c731122334400000000000000000000000000000000000000000000000000000001
//@ run-call: owner 0xffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: 0x666e1b390000000000000000000000010000000000000000000000000000000000000000

// Before Constantinople, ABI word validators check a shifted word, so a dirty argument still
// reverts. The failing calls pass raw calldata, since the encoder rejects out-of-range values.
contract LegacyValidators {
    function word8(uint8 x) external pure returns (uint256) {
        return x;
    }

    function flag(bool x) external pure returns (uint256) {
        return x ? 1 : 0;
    }

    function selector(bytes4 x) external pure returns (bytes4) {
        return x;
    }

    function owner(address x) external pure returns (address) {
        return x;
    }
}
