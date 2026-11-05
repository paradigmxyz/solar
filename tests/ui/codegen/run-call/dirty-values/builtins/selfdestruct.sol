//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Caddrp_0::t_local_selfdestruct
//@ run-call: Caddrp_0::t_retdirect_selfdestruct
//@ run-call: Caddrp_0::t_ret_selfdestruct
//@ run-call: Caddrp_0::t_param_selfdestruct
//@ run-call: Caddrp_0::t_copy_selfdestruct
//@ run-call: Caddrp_0::t_ternary_selfdestruct
//@ run-call: Caddrp_0::t_ifjoin_selfdestruct
//@ run-call: Caddrp_0::t_loopjoin_selfdestruct
//@ run-call: Caddrp_0::t_tuple_selfdestruct
//@ run-call: Caddrp_0::t_ptr_selfdestruct
//@ run-call: Caddrp_0::t_mem_selfdestruct

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

contract Caddrp_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_selfdestruct() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_selfdestruct() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_selfdestruct(address payable pv) internal   { selfdestruct(pv); }
    function _w_ptr_selfdestruct() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_selfdestruct() external   { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } selfdestruct(rx); }
    function t_retdirect_selfdestruct() external   { selfdestruct(_s_retdirect_selfdestruct()); }
    function t_ret_selfdestruct() external   { address payable rx = _s_ret_selfdestruct(); selfdestruct(rx); }
    function t_param_selfdestruct() external   { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } _p_param_selfdestruct(rx); }
    function t_copy_selfdestruct() external   { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable ry = rx; selfdestruct(ry); }
    function t_ternary_selfdestruct() external   { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address payable ro = payable(address(0x1111111111111111111111111111111111111111)); address payable ry = rf ? rx : ro; selfdestruct(ry); }
    function t_ifjoin_selfdestruct() external   { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address payable ro = payable(address(0x1111111111111111111111111111111111111111)); address payable ry; if (rf) { ry = rx; } else { ry = ro; } selfdestruct(ry); }
    function t_loopjoin_selfdestruct() external   { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable ry = payable(address(0x1111111111111111111111111111111111111111)); for (uint256 ri; ri < 1; ++ri) { ry = rx; } selfdestruct(ry); }
    function t_tuple_selfdestruct() external   { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address payable ry, uint256 rz) = (rx, 1); if (rz != 1) return; selfdestruct(ry); }
    function t_ptr_selfdestruct() external   { function() internal pure returns (uint256) rp = _w_ptr_selfdestruct; function() internal pure returns (address payable) rq; assembly { rq := rp } address payable ry = rq(); selfdestruct(ry); }
    function t_mem_selfdestruct() external   { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; selfdestruct(ry); }
}
