//@ codegen-matrix: standard
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: narrow 32, 0 => 1
//@ run-call: narrow 0, 0 => 1
//@ run-call: addresses 32, 0 => 1
//@ run-call: addresses 0, 0 => 1
//@ run-call: words 32, 0 => 1
//@ run-call: words 0, 0 => 1
//@ run-call-fail: narrow 128, 0
//@ run-call-fail: narrow 128, 1
//@ run-call-fail: narrow 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
//@ run-call-fail: addresses 128, 0
//@ run-call-fail: addresses 128, 1
//@ run-call-fail: addresses 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
//@ run-call-fail: words 128, 0
//@ run-call-fail: words 128, 1
//@ run-call-fail: words 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0

pragma solidity ^0.8.0;

// CHECK: @module

// Assembly constructs ranges that ABI decoding could not produce. We retain the copy range
// check for indexed arrays, including empty ranges. See CODEGEN-032 in docs/SOLC_DIVERGENCE.md.
contract ForgedCalldataEvents {
    event Narrow(uint8[] indexed values);
    event Addresses(address[] indexed values);
    event Words(uint256[] indexed values);

    function narrow(uint256 offset, uint256 length) external returns (uint256) {
        uint8[] calldata values;
        assembly {
            values.offset := offset
            values.length := length
        }
        emit Narrow(values);
        return 1;
    }

    function addresses(uint256 offset, uint256 length) external returns (uint256) {
        address[] calldata values;
        assembly {
            values.offset := offset
            values.length := length
        }
        emit Addresses(values);
        return 1;
    }

    function words(uint256 offset, uint256 length) external returns (uint256) {
        uint256[] calldata values;
        assembly {
            values.offset := offset
            values.length := length
        }
        emit Words(values);
        return 1;
    }
}
