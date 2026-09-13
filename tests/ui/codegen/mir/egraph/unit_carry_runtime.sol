//@ codegen-matrix: standard
//@ run-call: carries 0 => 0, 1
//@ run-call: carries 1 => 0, 0
//@ run-call: carries 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0, 0
//@ run-call: carries 0x8000000000000000000000000000000000000000000000000000000000000000 => 0, 0
//@ run-call: carries 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 0, 0
//@ run-call: carries 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 1, 0
//@ run-call: increment 0 => 1
//@ run-call: increment 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: increment 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: decrement 1 => 0
//@ run-call-fail: decrement 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: narrow 254 => 255
//@ run-call-fail: narrow 255 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract UnitCarry {
    function carries(uint256 x) external pure returns (uint256 carry, uint256 borrow) {
        assembly {
            carry := lt(add(x, 1), x)
            borrow := gt(sub(x, 1), x)
        }
    }

    function increment(uint256 x) external pure returns (uint256) {
        return x + 1;
    }

    function decrement(uint256 x) external pure returns (uint256) {
        return x - 1;
    }

    function narrow(uint8 x) external pure returns (uint8) {
        return x + 1;
    }
}
