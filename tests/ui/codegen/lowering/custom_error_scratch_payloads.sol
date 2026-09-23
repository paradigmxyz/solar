//@ codegen-matrix: standard
//@ run-call-fail: CustomErrorPayloads::none => Empty()()
//@ run-call-fail: CustomErrorPayloads::one 7 => One(uint256)(7)
//@ run-call-fail: CustomErrorPayloads::three -2, 0x61626364, true => Three(int8,bytes4,bool)(-2, 0x61626364, true)
//@ run-call-fail: CustomErrorPayloads::four 255 => Four(uint256,uint256,uint256,address)(1, 2, 3, 0x00000000000000000000000000000000000000ff)
//@ run-call-fail: CustomErrorPayloads::dynamic 5 => Dynamic(uint256,string)(5, "abc")
//@ run-call-fail: CustomErrorPayloads::required 9 => One(uint256)(9)
//@ run-call: CustomErrorPayloads::required 0
//@ run-call-fail: CustomErrorPayloads::allocateBelow 11 => One(uint256)(11)
//@ run-call: CustomErrorPayloads::allocateBelow 3 => 6

// Custom errors with up to three words stage their payload in the reserved low memory;
// longer or dynamic payloads encode at the free-memory pointer without reserving it.
contract CustomErrorPayloads {
    error Empty();
    error One(uint256 value);
    error Three(int8 small, bytes4 tag, bool flag);
    error Four(uint256 a, uint256 b, uint256 c, address who);
    error Dynamic(uint256 code, string message);

    function none() external pure {
        revert Empty();
    }

    function one(uint256 value) external pure {
        revert One(value);
    }

    function three(int8 small, bytes4 tag, bool flag) external pure {
        revert Three(small, tag, flag);
    }

    function four(uint160 who) external pure {
        revert Four(1, 2, 3, address(who));
    }

    function dynamic(uint256 code) external pure {
        revert Dynamic(code, "abc");
    }

    function required(uint256 value) external pure {
        require(value == 0, One(value));
    }

    // The failure no longer reads the free-memory pointer, while the success
    // path still allocates from it.
    function allocateBelow(uint256 n) external pure returns (uint256) {
        if (n > 10) revert One(n);
        uint256[] memory values = new uint256[](n);
        values[n - 1] = n;
        return values.length + values[n - 1];
    }
}
