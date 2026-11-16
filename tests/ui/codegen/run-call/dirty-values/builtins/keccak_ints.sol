//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_retdirect_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_ret_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_param_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_copy_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_ternary_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_ifjoin_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_loopjoin_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_tuple_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_modifier_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_ptr_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_mem_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_local_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_retdirect_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_ret_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_param_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_copy_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_ternary_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_ifjoin_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_loopjoin_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_tuple_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_modifier_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_ptr_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu8_0::t_mem_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cu64_0::t_local_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_retdirect_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_ret_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_param_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_copy_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_ternary_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_ifjoin_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_loopjoin_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_tuple_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_modifier_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_ptr_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_mem_keccak_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_local_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_retdirect_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_ret_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_param_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_copy_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_ternary_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_ifjoin_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_loopjoin_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_tuple_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_modifier_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_ptr_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Cu64_0::t_mem_keccak_packed_two => 90621073880681791100664928804300134053581271064688824920046354750743889210353
//@ run-call: Ci8_0::t_local_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_retdirect_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_ret_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_param_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_copy_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_ternary_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_ifjoin_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_loopjoin_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_tuple_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_modifier_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_ptr_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_mem_keccak_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_local_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_retdirect_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_ret_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_param_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_copy_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_ternary_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_ifjoin_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_loopjoin_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_tuple_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_modifier_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_ptr_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci8_0::t_mem_keccak_packed_two => 68455340876156084389494151788081960133053676831995537233425177884089478196892
//@ run-call: Ci24_0::t_local_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_retdirect_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_ret_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_param_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_copy_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_ternary_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_ifjoin_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_loopjoin_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_tuple_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_modifier_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_ptr_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_mem_keccak_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_local_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_retdirect_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_ret_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_param_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_copy_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_ternary_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_ifjoin_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_loopjoin_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_tuple_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_modifier_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_ptr_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265
//@ run-call: Ci24_0::t_mem_keccak_packed_two => 15169428025993894253458369488885129159823827802906612730201178358798107398265

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

contract Cu8_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_keccak_packed() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_keccak_packed(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(uint8 pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(uint8 rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_keccak_packed_two() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_keccak_packed_two() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_keccak_packed_two(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(uint8(3), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(uint8 pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(uint8(3), pv))); _; }
    function _e_modifier_keccak_packed_two(uint8 rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_keccak_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { uint8 rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodePacked(uint8(3), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(uint8(3), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { uint8 rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(uint8(3), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(keccak256(abi.encodePacked(uint8(3), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(uint8(3), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(uint8(3), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(uint8(3), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(uint8(3), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(keccak256(abi.encodePacked(uint8(3), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encodePacked(uint8(3), ry))); }
}
contract Cu64_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_ret_keccak_packed() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_keccak_packed(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(uint64 pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(uint64 rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0xabcd000000000000001b; }
    function _s_retdirect_keccak_packed_two() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_ret_keccak_packed_two() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_keccak_packed_two(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(uint64(3), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(uint64 pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(uint64(3), pv))); _; }
    function _e_modifier_keccak_packed_two(uint64 rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0xabcd000000000000001b; }
    function t_local_keccak_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { uint64 rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = uint64(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } (uint64 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (uint64) rq; assembly { rq := rp } uint64 ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encodePacked(uint64(3), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(uint64(3), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { uint64 rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(uint64(3), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = rx; return uint256(keccak256(abi.encodePacked(uint64(3), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(uint64(3), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(uint64(3), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = uint64(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(uint64(3), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } (uint64 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(uint64(3), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (uint64) rq; assembly { rq := rp } uint64 ry = rq(); return uint256(keccak256(abi.encodePacked(uint64(3), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encodePacked(uint64(3), ry))); }
}
contract Ci8_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _s_ret_keccak_packed() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_keccak_packed(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(int8 pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(int8 rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0xabcdfd; }
    function _s_retdirect_keccak_packed_two() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _s_ret_keccak_packed_two() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_keccak_packed_two(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(int8(-2), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(int8 pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(int8(-2), pv))); _; }
    function _e_modifier_keccak_packed_two(int8 rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0xabcdfd; }
    function t_local_keccak_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { int8 rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } bool rf = gasleft() > 0; int8 ro = int8(-2); int8 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } bool rf = gasleft() > 0; int8 ro = int8(-2); int8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 ry = int8(-2); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } (int8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (int8) rq; assembly { rq := rp } int8 ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encodePacked(int8(-2), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(int8(-2), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { int8 rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(int8(-2), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 ry = rx; return uint256(keccak256(abi.encodePacked(int8(-2), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } bool rf = gasleft() > 0; int8 ro = int8(-2); int8 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(int8(-2), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } bool rf = gasleft() > 0; int8 ro = int8(-2); int8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(int8(-2), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 ry = int8(-2); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(int8(-2), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } (int8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(int8(-2), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (int8) rq; assembly { rq := rp } int8 ry = rq(); return uint256(keccak256(abi.encodePacked(int8(-2), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encodePacked(int8(-2), ry))); }
}
contract Ci24_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _s_ret_keccak_packed() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_keccak_packed(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(int24 pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(int24 rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0xabcdfffffd; }
    function _s_retdirect_keccak_packed_two() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _s_ret_keccak_packed_two() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_keccak_packed_two(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(int24(-2), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(int24 pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(int24(-2), pv))); _; }
    function _e_modifier_keccak_packed_two(int24 rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0xabcdfffffd; }
    function t_local_keccak_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { int24 rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } bool rf = gasleft() > 0; int24 ro = int24(-2); int24 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } bool rf = gasleft() > 0; int24 ro = int24(-2); int24 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 ry = int24(-2); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } (int24 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (int24) rq; assembly { rq := rp } int24 ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encodePacked(int24(-2), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(int24(-2), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { int24 rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(int24(-2), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 ry = rx; return uint256(keccak256(abi.encodePacked(int24(-2), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } bool rf = gasleft() > 0; int24 ro = int24(-2); int24 ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(int24(-2), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } bool rf = gasleft() > 0; int24 ro = int24(-2); int24 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(int24(-2), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 ry = int24(-2); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(int24(-2), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } (int24 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(int24(-2), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (int24) rq; assembly { rq := rp } int24 ry = rq(); return uint256(keccak256(abi.encodePacked(int24(-2), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encodePacked(int24(-2), ry))); }
}
