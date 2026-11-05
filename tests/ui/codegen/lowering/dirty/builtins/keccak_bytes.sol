//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cb1_0::t_local_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_retdirect_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_ret_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_param_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_copy_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_ternary_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_ifjoin_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_loopjoin_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_tuple_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_modifier_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_ptr_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_mem_keccak_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_local_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_retdirect_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_ret_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_param_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_copy_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_ternary_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_ifjoin_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_loopjoin_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_tuple_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_modifier_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_ptr_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb1_0::t_mem_keccak_packed_two => 28758328303036651913838882008022681413928284653257947966664861242602317780178
//@ run-call: Cb4_0::t_local_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_retdirect_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_ret_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_param_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_copy_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_ternary_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_ifjoin_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_loopjoin_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_tuple_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_modifier_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_ptr_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_mem_keccak_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_local_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_retdirect_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_ret_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_param_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_copy_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_ternary_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_ifjoin_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_loopjoin_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_tuple_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_modifier_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_ptr_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb4_0::t_mem_keccak_packed_two => 60887822769710572777452583165576205630789147086855764935518707433434236812078
//@ run-call: Cb20_0::t_local_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_retdirect_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_ret_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_param_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_copy_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_ternary_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_ifjoin_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_loopjoin_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_tuple_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_modifier_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_ptr_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_mem_keccak_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_local_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_retdirect_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_ret_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_param_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_copy_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_ternary_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_ifjoin_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_loopjoin_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_tuple_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_modifier_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_ptr_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260
//@ run-call: Cb20_0::t_mem_keccak_packed_two => 18745691830388390915257531834354850294681960708646832047450259206215824867260

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

contract Cb1_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _s_ret_keccak_packed() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_keccak_packed(bytes1 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(bytes1 pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(bytes1 rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0xab0000000000000000000000000000000000000000000000000000000000ffff; }
    function _s_retdirect_keccak_packed_two() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _s_ret_keccak_packed_two() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_keccak_packed_two(bytes1 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes1(0xcd), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(bytes1 pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(bytes1(0xcd), pv))); _; }
    function _e_modifier_keccak_packed_two(bytes1 rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0xab0000000000000000000000000000000000000000000000000000000000ffff; }
    function t_local_keccak_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { bytes1 rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bool rf = gasleft() > 0; bytes1 ro = bytes1(0xcd); bytes1 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bool rf = gasleft() > 0; bytes1 ro = bytes1(0xcd); bytes1 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 ry = bytes1(0xcd); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } (bytes1 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (bytes1) rq; assembly { rq := rp } bytes1 ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(keccak256(abi.encodePacked(bytes1(0xcd), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes1(0xcd), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { bytes1 rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(bytes1(0xcd), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 ry = rx; return uint256(keccak256(abi.encodePacked(bytes1(0xcd), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bool rf = gasleft() > 0; bytes1 ro = bytes1(0xcd); bytes1 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(bytes1(0xcd), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bool rf = gasleft() > 0; bytes1 ro = bytes1(0xcd); bytes1 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(bytes1(0xcd), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 ry = bytes1(0xcd); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(bytes1(0xcd), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } (bytes1 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(bytes1(0xcd), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (bytes1) rq; assembly { rq := rp } bytes1 ry = rq(); return uint256(keccak256(abi.encodePacked(bytes1(0xcd), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(keccak256(abi.encodePacked(bytes1(0xcd), ry))); }
}
contract Cb4_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_keccak_packed() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_keccak_packed(bytes4 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(bytes4 pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(bytes4 rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_keccak_packed_two() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_keccak_packed_two() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_keccak_packed_two(bytes4 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(bytes4 pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(bytes4(0x11223344), pv))); _; }
    function _e_modifier_keccak_packed_two(bytes4 rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_local_keccak_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { bytes4 rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { bytes4 rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), ry))); }
}
contract Cb20_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _s_ret_keccak_packed() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_keccak_packed(bytes20 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(bytes20 pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(bytes20 rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0x123456789012345678901234567890123456789000000000000000000000abcd; }
    function _s_retdirect_keccak_packed_two() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _s_ret_keccak_packed_two() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_keccak_packed_two(bytes20 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(bytes20 pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), pv))); _; }
    function _e_modifier_keccak_packed_two(bytes20 rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0x123456789012345678901234567890123456789000000000000000000000abcd; }
    function t_local_keccak_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { bytes20 rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bool rf = gasleft() > 0; bytes20 ro = bytes20(0x1111111111111111111111111111111111111111); bytes20 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bool rf = gasleft() > 0; bytes20 ro = bytes20(0x1111111111111111111111111111111111111111); bytes20 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 ry = bytes20(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } (bytes20 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (bytes20) rq; assembly { rq := rp } bytes20 ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { bytes20 rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 ry = rx; return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bool rf = gasleft() > 0; bytes20 ro = bytes20(0x1111111111111111111111111111111111111111); bytes20 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bool rf = gasleft() > 0; bytes20 ro = bytes20(0x1111111111111111111111111111111111111111); bytes20 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 ry = bytes20(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } (bytes20 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (bytes20) rq; assembly { rq := rp } bytes20 ry = rq(); return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), ry))); }
}
