//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: Caddr_0::t_local_error_revert => 0x445a9b5b0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_retdirect_error_revert => 0x445a9b5b0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_param_error_revert => 0x445a9b5b0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_mem_error_revert => 0x445a9b5b0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_local_error_require => 0x445a9b5b0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_retdirect_error_require => 0x445a9b5b0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_param_error_require => 0x445a9b5b0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_mem_error_require => 0x445a9b5b0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_local_error_two => 0x7f7558bd00000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_retdirect_error_two => 0x7f7558bd00000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_param_error_two => 0x7f7558bd00000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_mem_error_two => 0x7f7558bd00000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_local_error_struct => 0xd7948ca300000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_retdirect_error_struct => 0xd7948ca300000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_param_error_struct => 0xd7948ca300000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddr_0::t_mem_error_struct => 0xd7948ca300000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_local_error_revert => 0x5e64e0c10000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_retdirect_error_revert => 0x5e64e0c10000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_param_error_revert => 0x5e64e0c10000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_mem_error_revert => 0x5e64e0c10000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_local_error_require => 0x5e64e0c10000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_retdirect_error_require => 0x5e64e0c10000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_param_error_require => 0x5e64e0c10000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_mem_error_require => 0x5e64e0c10000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_local_error_two => 0xcaf3b58700000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_retdirect_error_two => 0xcaf3b58700000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_param_error_two => 0xcaf3b58700000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_mem_error_two => 0xcaf3b58700000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_local_error_struct => 0x75851bb400000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_retdirect_error_struct => 0x75851bb400000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_param_error_struct => 0x75851bb400000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Caddrp_0::t_mem_error_struct => 0x75851bb400000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_local_error_revert => 0xf5285c2d0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_retdirect_error_revert => 0xf5285c2d0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_param_error_revert => 0xf5285c2d0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_mem_error_revert => 0xf5285c2d0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_local_error_require => 0xf5285c2d0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_retdirect_error_require => 0xf5285c2d0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_param_error_require => 0xf5285c2d0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_mem_error_require => 0xf5285c2d0000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_local_error_two => 0x51b2b9c000000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_retdirect_error_two => 0x51b2b9c000000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_param_error_two => 0x51b2b9c000000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_mem_error_two => 0x51b2b9c000000000000000000000000011111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_local_error_struct => 0x496bedf900000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_retdirect_error_struct => 0x496bedf900000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_param_error_struct => 0x496bedf900000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890
//@ run-call-fail: Cctr_0::t_mem_error_struct => 0x496bedf900000000000000000000000000000000000000000000000000000000000000010000000000000000000000001234567890123456789012345678901234567890

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: ABI encodings and custom errors.

interface IC {
    function f() external;
}

enum E {
    A,
    B,
    C
}

type U is uint8;

struct St_addr {
    uint256 a;
    address f;
}

error Err_addr(address v);
error Err2_addr(address a, address b);

error ErrS_addr(St_addr s);

struct St_addrp {
    uint256 a;
    address payable f;
}

error Err_addrp(address payable v);
error Err2_addrp(address payable a, address payable b);

error ErrS_addrp(St_addrp s);

struct St_ctr {
    uint256 a;
    IC f;
}

error Err_ctr(IC v);
error Err2_ctr(IC a, IC b);

error ErrS_ctr(St_ctr s);

contract Caddr_0 {
    function _s_retdirect_error_revert() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_revert(address pv) internal view  returns (uint256) { revert Err_addr(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_require(address pv) internal view  returns (uint256) { require(gasleft() == 0, Err_addr(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_two(address pv) internal view  returns (uint256) { revert Err2_addr(address(0x1111111111111111111111111111111111111111), pv); return 0; }
    function _s_retdirect_error_struct() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_struct(address pv) internal view  returns (uint256) { revert ErrS_addr(St_addr(1, pv)); return 0; }
    function t_local_error_revert() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } revert Err_addr(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_addr(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; revert Err_addr(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } require(gasleft() == 0, Err_addr(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_addr(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; require(gasleft() == 0, Err_addr(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } revert Err2_addr(address(0x1111111111111111111111111111111111111111), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_addr(address(0x1111111111111111111111111111111111111111), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; revert Err2_addr(address(0x1111111111111111111111111111111111111111), ry); return 0; }
    function t_local_error_struct() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } revert ErrS_addr(St_addr(1, rx)); return 0; }
    function t_retdirect_error_struct() external view  returns (uint256) { revert ErrS_addr(St_addr(1, _s_retdirect_error_struct())); return 0; }
    function t_param_error_struct() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_struct(rx); }
    function t_mem_error_struct() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; revert ErrS_addr(St_addr(1, ry)); return 0; }
}
contract Caddrp_0 {
    function _s_retdirect_error_revert() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_revert(address payable pv) internal view  returns (uint256) { revert Err_addrp(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_require(address payable pv) internal view  returns (uint256) { require(gasleft() == 0, Err_addrp(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_two(address payable pv) internal view  returns (uint256) { revert Err2_addrp(payable(address(0x1111111111111111111111111111111111111111)), pv); return 0; }
    function _s_retdirect_error_struct() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_struct(address payable pv) internal view  returns (uint256) { revert ErrS_addrp(St_addrp(1, pv)); return 0; }
    function t_local_error_revert() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } revert Err_addrp(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_addrp(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; revert Err_addrp(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } require(gasleft() == 0, Err_addrp(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_addrp(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; require(gasleft() == 0, Err_addrp(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } revert Err2_addrp(payable(address(0x1111111111111111111111111111111111111111)), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_addrp(payable(address(0x1111111111111111111111111111111111111111)), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; revert Err2_addrp(payable(address(0x1111111111111111111111111111111111111111)), ry); return 0; }
    function t_local_error_struct() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } revert ErrS_addrp(St_addrp(1, rx)); return 0; }
    function t_retdirect_error_struct() external view  returns (uint256) { revert ErrS_addrp(St_addrp(1, _s_retdirect_error_struct())); return 0; }
    function t_param_error_struct() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_struct(rx); }
    function t_mem_error_struct() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; revert ErrS_addrp(St_addrp(1, ry)); return 0; }
}
contract Cctr_0 {
    function _s_retdirect_error_revert() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_revert(IC pv) internal view  returns (uint256) { revert Err_ctr(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_require(IC pv) internal view  returns (uint256) { require(gasleft() == 0, Err_ctr(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_two(IC pv) internal view  returns (uint256) { revert Err2_ctr(IC(address(0x1111111111111111111111111111111111111111)), pv); return 0; }
    function _s_retdirect_error_struct() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_error_struct(IC pv) internal view  returns (uint256) { revert ErrS_ctr(St_ctr(1, pv)); return 0; }
    function t_local_error_revert() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } revert Err_ctr(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_ctr(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; revert Err_ctr(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } require(gasleft() == 0, Err_ctr(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_ctr(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; require(gasleft() == 0, Err_ctr(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } revert Err2_ctr(IC(address(0x1111111111111111111111111111111111111111)), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_ctr(IC(address(0x1111111111111111111111111111111111111111)), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; revert Err2_ctr(IC(address(0x1111111111111111111111111111111111111111)), ry); return 0; }
    function t_local_error_struct() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } revert ErrS_ctr(St_ctr(1, rx)); return 0; }
    function t_retdirect_error_struct() external view  returns (uint256) { revert ErrS_ctr(St_ctr(1, _s_retdirect_error_struct())); return 0; }
    function t_param_error_struct() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_error_struct(rx); }
    function t_mem_error_struct() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; revert ErrS_ctr(St_ctr(1, ry)); return 0; }
}
