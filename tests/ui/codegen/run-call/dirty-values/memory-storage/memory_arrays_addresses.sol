//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Caddr_0::t_local_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_mnested => 103929005307130220006098923584552504982110632080

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: memory and storage stores, keys, indexes, slices and allocations.

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
    function _s_retdirect_marray_elem() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_elem(address pv) internal view  returns (uint256) { address[] memory a = new address[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_fixed(address pv) internal view  returns (uint256) { address[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal(address pv) internal view  returns (uint256) { address[2] memory a = [pv, address(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal_dyn(address pv) internal view  returns (uint256) { address[] memory a = new address[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mnested(address pv) internal view  returns (uint256) { address[][] memory a = new address[][](1); a[0] = new address[](1); a[0][0] = pv; address[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[] memory a = new address[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { address[] memory a = new address[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[] memory a = new address[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { address[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[2] memory a = [rx, address(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { address[2] memory a = [_s_retdirect_marray_literal(), address(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[2] memory a = [ry, address(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[] memory a = new address[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { address[] memory a = new address[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[] memory a = new address[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[][] memory a = new address[][](1); a[0] = new address[](1); a[0][0] = rx; address[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { address[][] memory a = new address[][](1); a[0] = new address[](1); a[0][0] = _s_retdirect_mnested(); address[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[][] memory a = new address[][](1); a[0] = new address[](1); a[0][0] = ry; address[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
contract Caddrp_0 {
    function _s_retdirect_marray_elem() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_elem(address payable pv) internal view  returns (uint256) { address payable[] memory a = new address payable[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_fixed(address payable pv) internal view  returns (uint256) { address payable[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal(address payable pv) internal view  returns (uint256) { address payable[2] memory a = [pv, payable(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal_dyn(address payable pv) internal view  returns (uint256) { address payable[] memory a = new address payable[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mnested(address payable pv) internal view  returns (uint256) { address payable[][] memory a = new address payable[][](1); a[0] = new address payable[](1); a[0][0] = pv; address payable[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[] memory a = new address payable[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { address payable[] memory a = new address payable[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[] memory a = new address payable[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { address payable[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[2] memory a = [rx, payable(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { address payable[2] memory a = [_s_retdirect_marray_literal(), payable(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[2] memory a = [ry, payable(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[] memory a = new address payable[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { address payable[] memory a = new address payable[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[] memory a = new address payable[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[][] memory a = new address payable[][](1); a[0] = new address payable[](1); a[0][0] = rx; address payable[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { address payable[][] memory a = new address payable[][](1); a[0] = new address payable[](1); a[0][0] = _s_retdirect_mnested(); address payable[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[][] memory a = new address payable[][](1); a[0] = new address payable[](1); a[0][0] = ry; address payable[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
contract Cctr_0 {
    function _s_retdirect_marray_elem() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_elem(IC pv) internal view  returns (uint256) { IC[] memory a = new IC[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_fixed(IC pv) internal view  returns (uint256) { IC[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal(IC pv) internal view  returns (uint256) { IC[2] memory a = [pv, IC(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal_dyn(IC pv) internal view  returns (uint256) { IC[] memory a = new IC[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mnested(IC pv) internal view  returns (uint256) { IC[][] memory a = new IC[][](1); a[0] = new IC[](1); a[0][0] = pv; IC[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[] memory a = new IC[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { IC[] memory a = new IC[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[] memory a = new IC[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { IC[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[2] memory a = [rx, IC(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { IC[2] memory a = [_s_retdirect_marray_literal(), IC(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[2] memory a = [ry, IC(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[] memory a = new IC[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { IC[] memory a = new IC[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[] memory a = new IC[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[][] memory a = new IC[][](1); a[0] = new IC[](1); a[0][0] = rx; IC[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { IC[][] memory a = new IC[][](1); a[0] = new IC[](1); a[0][0] = _s_retdirect_mnested(); IC[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[][] memory a = new IC[][](1); a[0] = new IC[](1); a[0][0] = ry; IC[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
