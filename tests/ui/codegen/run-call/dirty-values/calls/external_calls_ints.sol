//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_ext_arg => 27
//@ run-call: Cu8_0::t_retdirect_ext_arg => 27
//@ run-call: Cu8_0::t_param_ext_arg => 27
//@ run-call: Cu8_0::t_mem_ext_arg => 27
//@ run-call: Cu8_0::t_local_ext_arg_second => 27
//@ run-call: Cu8_0::t_retdirect_ext_arg_second => 27
//@ run-call: Cu8_0::t_param_ext_arg_second => 27
//@ run-call: Cu8_0::t_mem_ext_arg_second => 27
//@ run-call: Cu8_0::t_local_ext_try => 27
//@ run-call: Cu8_0::t_retdirect_ext_try => 27
//@ run-call: Cu8_0::t_param_ext_try => 27
//@ run-call: Cu8_0::t_mem_ext_try => 27
//@ run-call: Cu8_0::t_local_ext_try_catch_arg => 0
//@ run-call: Cu8_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Cu8_0::t_param_ext_try_catch_arg => 0
//@ run-call: Cu8_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Cu8_0::t_local_ext_static => 27
//@ run-call: Cu8_0::t_retdirect_ext_static => 27
//@ run-call: Cu8_0::t_param_ext_static => 27
//@ run-call: Cu8_0::t_mem_ext_static => 27
//@ run-call: Cu8_0::t_local_ext_lowlevel_encode => 27
//@ run-call: Cu8_0::t_retdirect_ext_lowlevel_encode => 27
//@ run-call: Cu8_0::t_param_ext_lowlevel_encode => 27
//@ run-call: Cu8_0::t_mem_ext_lowlevel_encode => 27
//@ run-call: Cu8_0::t_local_ext_new_arg => 27
//@ run-call: Cu8_0::t_retdirect_ext_new_arg => 27
//@ run-call: Cu8_0::t_param_ext_new_arg => 27
//@ run-call: Cu8_0::t_mem_ext_new_arg => 27
//@ run-call: Cu64_0::t_local_ext_arg => 27
//@ run-call: Cu64_0::t_retdirect_ext_arg => 27
//@ run-call: Cu64_0::t_param_ext_arg => 27
//@ run-call: Cu64_0::t_mem_ext_arg => 27
//@ run-call: Cu64_0::t_local_ext_arg_second => 27
//@ run-call: Cu64_0::t_retdirect_ext_arg_second => 27
//@ run-call: Cu64_0::t_param_ext_arg_second => 27
//@ run-call: Cu64_0::t_mem_ext_arg_second => 27
//@ run-call: Cu64_0::t_local_ext_try => 27
//@ run-call: Cu64_0::t_retdirect_ext_try => 27
//@ run-call: Cu64_0::t_param_ext_try => 27
//@ run-call: Cu64_0::t_mem_ext_try => 27
//@ run-call: Cu64_0::t_local_ext_try_catch_arg => 0
//@ run-call: Cu64_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Cu64_0::t_param_ext_try_catch_arg => 0
//@ run-call: Cu64_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Cu64_0::t_local_ext_static => 27
//@ run-call: Cu64_0::t_retdirect_ext_static => 27
//@ run-call: Cu64_0::t_param_ext_static => 27
//@ run-call: Cu64_0::t_mem_ext_static => 27
//@ run-call: Cu64_0::t_local_ext_lowlevel_encode => 27
//@ run-call: Cu64_0::t_retdirect_ext_lowlevel_encode => 27
//@ run-call: Cu64_0::t_param_ext_lowlevel_encode => 27
//@ run-call: Cu64_0::t_mem_ext_lowlevel_encode => 27
//@ run-call: Cu64_0::t_local_ext_new_arg => 27
//@ run-call: Cu64_0::t_retdirect_ext_new_arg => 27
//@ run-call: Cu64_0::t_param_ext_new_arg => 27
//@ run-call: Cu64_0::t_mem_ext_new_arg => 27
//@ run-call: Ci8_0::t_local_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ext_try_catch_arg => 0
//@ run-call: Ci8_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Ci8_0::t_param_ext_try_catch_arg => 0
//@ run-call: Ci8_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Ci8_0::t_local_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_try_catch_arg => 0
//@ run-call: Ci24_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Ci24_0::t_param_ext_try_catch_arg => 0
//@ run-call: Ci24_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Ci24_0::t_local_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: external calls, try/catch, call options, constructors and address operands.

interface IC {
    function f() external;
}

enum E {
    A,
    B,
    C
}

type U is uint8;

contract Callee_u8 {
    function f(uint8 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(uint8 a, uint8 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(uint8 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Callee_u64 {
    function f(uint64 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(uint64 a, uint64 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(uint64 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Callee_i8 {
    function f(int8 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(int8 a, int8 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(int8 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Callee_i24 {
    function f(int24 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(int24 a, int24 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(int24 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Cu8_0 {
    function _s_retdirect_ext_arg() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_arg(uint8 pv) internal   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_arg_second(uint8 pv) internal   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f2(uint8(3), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_try(uint8 pv) internal   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_try_catch_arg(uint8 pv) internal   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_static(uint8 pv) internal   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_lowlevel_encode(uint8 pv) internal   returns (uint256) { Callee_u8 c = new Callee_u8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u8.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_new_arg(uint8 pv) internal   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.f2(uint8(3), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f2(uint8(3), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.f2(uint8(3), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u8.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_u8 c = new Callee_u8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u8.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u8.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.f(ry); return r; }
}
contract Cu64_0 {
    function _s_retdirect_ext_arg() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_arg(uint64 pv) internal   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_arg_second(uint64 pv) internal   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f2(uint64(3), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_try(uint64 pv) internal   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_try_catch_arg(uint64 pv) internal   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_static(uint64 pv) internal   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_lowlevel_encode(uint64 pv) internal   returns (uint256) { Callee_u64 c = new Callee_u64(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u64.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_new_arg(uint64 pv) internal   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.f2(uint64(3), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f2(uint64(3), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.f2(uint64(3), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u64.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_u64 c = new Callee_u64(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u64.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u64.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.f(ry); return r; }
}
contract Ci8_0 {
    function _s_retdirect_ext_arg() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_arg(int8 pv) internal   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_arg_second(int8 pv) internal   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f2(int8(-2), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_try(int8 pv) internal   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_try_catch_arg(int8 pv) internal   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_static(int8 pv) internal   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_lowlevel_encode(int8 pv) internal   returns (uint256) { Callee_i8 c = new Callee_i8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i8.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_new_arg(int8 pv) internal   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r = c.f2(int8(-2), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f2(int8(-2), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r = c.f2(int8(-2), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i8.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_i8 c = new Callee_i8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i8.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i8.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r = c.f(ry); return r; }
}
contract Ci24_0 {
    function _s_retdirect_ext_arg() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_arg(int24 pv) internal   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_arg_second(int24 pv) internal   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f2(int24(-2), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_try(int24 pv) internal   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_try_catch_arg(int24 pv) internal   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_static(int24 pv) internal   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_lowlevel_encode(int24 pv) internal   returns (uint256) { Callee_i24 c = new Callee_i24(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i24.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_new_arg(int24 pv) internal   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r = c.f2(int24(-2), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f2(int24(-2), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r = c.f2(int24(-2), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i24.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_i24 c = new Callee_i24(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i24.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i24.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r = c.f(ry); return r; }
}
