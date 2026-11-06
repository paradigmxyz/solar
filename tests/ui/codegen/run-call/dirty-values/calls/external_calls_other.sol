//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cbool_0::t_local_ext_arg => 1
//@ run-call: Cbool_0::t_retdirect_ext_arg => 1
//@ run-call: Cbool_0::t_param_ext_arg => 1
//@ run-call: Cbool_0::t_mem_ext_arg => 1
//@ run-call: Cbool_0::t_local_ext_arg_second => 1
//@ run-call: Cbool_0::t_retdirect_ext_arg_second => 1
//@ run-call: Cbool_0::t_param_ext_arg_second => 1
//@ run-call: Cbool_0::t_mem_ext_arg_second => 1
//@ run-call: Cbool_0::t_local_ext_try => 1
//@ run-call: Cbool_0::t_retdirect_ext_try => 1
//@ run-call: Cbool_0::t_param_ext_try => 1
//@ run-call: Cbool_0::t_mem_ext_try => 1
//@ run-call: Cbool_0::t_local_ext_try_catch_arg => 0
//@ run-call: Cbool_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Cbool_0::t_param_ext_try_catch_arg => 0
//@ run-call: Cbool_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Cbool_0::t_local_ext_static => 1
//@ run-call: Cbool_0::t_retdirect_ext_static => 1
//@ run-call: Cbool_0::t_param_ext_static => 1
//@ run-call: Cbool_0::t_mem_ext_static => 1
//@ run-call: Cbool_0::t_local_ext_lowlevel_encode => 1
//@ run-call: Cbool_0::t_retdirect_ext_lowlevel_encode => 1
//@ run-call: Cbool_0::t_param_ext_lowlevel_encode => 1
//@ run-call: Cbool_0::t_mem_ext_lowlevel_encode => 1
//@ run-call: Cbool_0::t_local_ext_new_arg => 1
//@ run-call: Cbool_0::t_retdirect_ext_new_arg => 1
//@ run-call: Cbool_0::t_param_ext_new_arg => 1
//@ run-call: Cbool_0::t_mem_ext_new_arg => 1
//@ run-call-fail: Cen_0::t_local_ext_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_ext_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_ext_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_ext_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_ext_arg_second => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_ext_arg_second => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_ext_arg_second => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_ext_arg_second => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_ext_try => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_ext_try => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_ext_try => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_ext_try => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_ext_try_catch_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_ext_try_catch_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_ext_try_catch_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_ext_try_catch_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_ext_static => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_ext_static => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_ext_static => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_ext_static => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_ext_lowlevel_encode => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_ext_lowlevel_encode => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_ext_lowlevel_encode => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_ext_lowlevel_encode => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_ext_new_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_ext_new_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_ext_new_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_ext_new_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_ext_arg => 27
//@ run-call: Cud_0::t_retdirect_ext_arg => 27
//@ run-call: Cud_0::t_param_ext_arg => 27
//@ run-call: Cud_0::t_mem_ext_arg => 27
//@ run-call: Cud_0::t_local_ext_arg_second => 27
//@ run-call: Cud_0::t_retdirect_ext_arg_second => 27
//@ run-call: Cud_0::t_param_ext_arg_second => 27
//@ run-call: Cud_0::t_mem_ext_arg_second => 27
//@ run-call: Cud_0::t_local_ext_try => 27
//@ run-call: Cud_0::t_retdirect_ext_try => 27
//@ run-call: Cud_0::t_param_ext_try => 27
//@ run-call: Cud_0::t_mem_ext_try => 27
//@ run-call: Cud_0::t_local_ext_try_catch_arg => 0
//@ run-call: Cud_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Cud_0::t_param_ext_try_catch_arg => 0
//@ run-call: Cud_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Cud_0::t_local_ext_static => 27
//@ run-call: Cud_0::t_retdirect_ext_static => 27
//@ run-call: Cud_0::t_param_ext_static => 27
//@ run-call: Cud_0::t_mem_ext_static => 27
//@ run-call: Cud_0::t_local_ext_lowlevel_encode => 27
//@ run-call: Cud_0::t_retdirect_ext_lowlevel_encode => 27
//@ run-call: Cud_0::t_param_ext_lowlevel_encode => 27
//@ run-call: Cud_0::t_mem_ext_lowlevel_encode => 27
//@ run-call: Cud_0::t_local_ext_new_arg => 27
//@ run-call: Cud_0::t_retdirect_ext_new_arg => 27
//@ run-call: Cud_0::t_param_ext_new_arg => 27
//@ run-call: Cud_0::t_mem_ext_new_arg => 27

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

contract Callee_bool {
    function f(bool v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(bool a, bool b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(bool v) external pure {
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

contract Callee_en {
    function f(E v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(E a, E b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(E v) external pure {
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

contract Callee_ud {
    function f(U v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(U a, U b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(U v) external pure {
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

contract Cbool_0 {
    function _s_retdirect_ext_arg() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_arg(bool pv) internal   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_arg_second(bool pv) internal   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f2(false, pv); return r; }
    function _s_retdirect_ext_try() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_try(bool pv) internal   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_try_catch_arg(bool pv) internal   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_static(bool pv) internal   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_lowlevel_encode(bool pv) internal   returns (uint256) { Callee_bool c = new Callee_bool(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_bool.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_new_arg(bool pv) internal   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r = c.f2(false, rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f2(false, _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r = c.f2(false, ry); return r; }
    function t_local_ext_try() external   returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_bool.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_bool c = new Callee_bool(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_bool.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_bool.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r = c.f(ry); return r; }
}
contract Cen_0 {
    function _s_retdirect_ext_arg() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_arg(E pv) internal   returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_arg_second(E pv) internal   returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f2(E.A, pv); return r; }
    function _s_retdirect_ext_try() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_try(E pv) internal   returns (uint256) { Callee_en c = new Callee_en(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_try_catch_arg(E pv) internal   returns (uint256) { Callee_en c = new Callee_en(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_static(E pv) internal   returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_lowlevel_encode(E pv) internal   returns (uint256) { Callee_en c = new Callee_en(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_en.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_new_arg(E pv) internal   returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r = c.f2(E.A, rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f2(E.A, _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r = c.f2(E.A, ry); return r; }
    function t_local_ext_try() external   returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_en c = new Callee_en(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_en c = new Callee_en(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_en.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_en c = new Callee_en(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_en.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_en.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r = c.f(ry); return r; }
}
contract Cud_0 {
    function _s_retdirect_ext_arg() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_arg(U pv) internal   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_arg_second(U pv) internal   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f2(U.wrap(3), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_try(U pv) internal   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_try_catch_arg(U pv) internal   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_static(U pv) internal   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_lowlevel_encode(U pv) internal   returns (uint256) { Callee_ud c = new Callee_ud(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ud.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_new_arg(U pv) internal   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r = c.f2(U.wrap(3), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f2(U.wrap(3), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r = c.f2(U.wrap(3), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ud.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_ud c = new Callee_ud(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ud.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ud.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r = c.f(ry); return r; }
}
