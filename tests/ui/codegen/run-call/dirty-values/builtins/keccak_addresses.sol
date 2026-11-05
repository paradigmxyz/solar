//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Caddr_0::t_local_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_retdirect_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_ret_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_param_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_copy_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_ternary_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_ifjoin_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_loopjoin_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_tuple_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_modifier_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_ptr_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_mem_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_local_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_retdirect_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_ret_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_param_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_copy_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_ternary_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_ifjoin_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_loopjoin_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_tuple_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_modifier_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_ptr_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddr_0::t_mem_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_local_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_retdirect_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_ret_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_param_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_copy_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_ternary_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_ifjoin_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_loopjoin_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_tuple_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_modifier_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_ptr_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_mem_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_local_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_retdirect_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_ret_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_param_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_copy_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_ternary_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_ifjoin_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_loopjoin_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_tuple_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_modifier_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_ptr_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Caddrp_0::t_mem_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_local_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_retdirect_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_ret_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_param_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_copy_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_ternary_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_ifjoin_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_loopjoin_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_tuple_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_modifier_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_ptr_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_mem_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_local_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_retdirect_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_ret_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_param_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_copy_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_ternary_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_ifjoin_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_loopjoin_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_tuple_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_modifier_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_ptr_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cctr_0::t_mem_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: builtins with value parameters.

interface IC {
    function f() external;
}

enum E {
    A,
    B,
    C
}

type U is uint8;

contract Caddr_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_keccak_packed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_keccak_packed(address pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(address pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(address rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_keccak_packed_two() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_keccak_packed_two() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_keccak_packed_two(address pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(address pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), pv))); _; }
    function _e_modifier_keccak_packed_two(address rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_keccak_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { address rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { address rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), ry))); }
}
contract Caddrp_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_keccak_packed() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_keccak_packed(address payable pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(address payable pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(address payable rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_keccak_packed_two() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_keccak_packed_two() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_keccak_packed_two(address payable pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(address payable pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), pv))); _; }
    function _e_modifier_keccak_packed_two(address payable rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_keccak_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { address payable rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address payable ro = payable(address(0x1111111111111111111111111111111111111111)); address payable ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address payable ro = payable(address(0x1111111111111111111111111111111111111111)); address payable ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable ry = payable(address(0x1111111111111111111111111111111111111111)); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address payable ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (address payable) rq; assembly { rq := rp } address payable ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { address payable rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable ry = rx; return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address payable ro = payable(address(0x1111111111111111111111111111111111111111)); address payable ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address payable ro = payable(address(0x1111111111111111111111111111111111111111)); address payable ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable ry = payable(address(0x1111111111111111111111111111111111111111)); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address payable ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (address payable) rq; assembly { rq := rp } address payable ry = rq(); return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), ry))); }
}
contract Cctr_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_keccak_packed() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_keccak_packed(IC pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(IC pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(IC rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_keccak_packed_two() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_keccak_packed_two() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_keccak_packed_two(IC pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(IC pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), pv))); _; }
    function _e_modifier_keccak_packed_two(IC rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_keccak_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { IC rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; IC ro = IC(address(0x1111111111111111111111111111111111111111)); IC ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; IC ro = IC(address(0x1111111111111111111111111111111111111111)); IC ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC ry = IC(address(0x1111111111111111111111111111111111111111)); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (IC ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (IC) rq; assembly { rq := rp } IC ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { IC rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC ry = rx; return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; IC ro = IC(address(0x1111111111111111111111111111111111111111)); IC ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; IC ro = IC(address(0x1111111111111111111111111111111111111111)); IC ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC ry = IC(address(0x1111111111111111111111111111111111111111)); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (IC ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (IC) rq; assembly { rq := rp } IC ry = rq(); return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), ry))); }
}
