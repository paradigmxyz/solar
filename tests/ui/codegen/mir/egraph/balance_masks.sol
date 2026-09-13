//@ codegen-matrix: standard
//@ run-call: masked 0 => 0
//@ run-call: masked 0x10000000000000000000000000000000000000000 => 0
//@ run-call: wide 0x10000000000000000000000000000000000000000 => 0
//@ run-call: shared 0x10000000000000000000000000000000000000000 => 0, 0
//@ run-call: funded; value=17 => 17, 17, 17

contract BalanceMasks {
    function masked(uint256 account) public view returns (uint256 value) {
        assembly { value := balance(and(account, 0xffffffffffffffffffffffffffffffffffffffff)) }
    }

    function wide(uint256 account) public view returns (uint256 value) {
        assembly { value := balance(and(0x800000000000000000000000ffffffffffffffffffffffffffffffffffffffff, account)) }
    }

    function shared(uint256 account) public view returns (uint256 value, uint256 clean) {
        assembly {
            clean := and(account, 0xffffffffffffffffffffffffffffffffffffffff)
            value := balance(clean)
        }
    }

    function funded() external payable returns (uint256, uint256, uint256) {
        uint256 dirty = uint256(uint160(address(this))) | (uint256(1) << 200);
        (uint256 value, uint256 clean) = shared(dirty);
        assert(clean == uint256(uint160(address(this))));
        return (masked(dirty), wide(dirty), value);
    }
}
