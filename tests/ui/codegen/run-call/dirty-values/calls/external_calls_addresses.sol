//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Caddr_0::t_local_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_ext_try_catch_arg => 0
//@ run-call: Caddr_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Caddr_0::t_param_ext_try_catch_arg => 0
//@ run-call: Caddr_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Caddr_0::t_local_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_ext_try_catch_arg => 0
//@ run-call: Caddrp_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Caddrp_0::t_param_ext_try_catch_arg => 0
//@ run-call: Caddrp_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Caddrp_0::t_local_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_ext_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_ext_arg_second => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_ext_try => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_ext_try_catch_arg => 0
//@ run-call: Cctr_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Cctr_0::t_param_ext_try_catch_arg => 0
//@ run-call: Cctr_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Cctr_0::t_local_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_ext_static => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_ext_lowlevel_encode => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_ext_new_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_ext_new_arg => 103929005307130220006098923584552504982110632080

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

contract Callee_addr {
    function f(address v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(address a, address b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(address v) external pure {
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

contract Callee_addrp {
    function f(address payable v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(address payable a, address payable b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(address payable v) external pure {
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

contract Callee_ctr {
    function f(IC v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(IC a, IC b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(IC v) external pure {
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

contract Caddr_0 {
    function _s_retdirect_ext_arg() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg(address pv) internal   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg_second(address pv) internal   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f2(address(0x1111111111111111111111111111111111111111), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try(address pv) internal   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try_catch_arg(address pv) internal   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_static(address pv) internal   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_lowlevel_encode(address pv) internal   returns (uint256) { Callee_addr c = new Callee_addr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addr.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_new_arg(address pv) internal   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r = c.f2(address(0x1111111111111111111111111111111111111111), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f2(address(0x1111111111111111111111111111111111111111), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r = c.f2(address(0x1111111111111111111111111111111111111111), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addr.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_addr c = new Callee_addr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addr.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addr.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r = c.f(ry); return r; }
}
contract Caddrp_0 {
    function _s_retdirect_ext_arg() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg(address payable pv) internal   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg_second(address payable pv) internal   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f2(payable(address(0x1111111111111111111111111111111111111111)), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try(address payable pv) internal   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try_catch_arg(address payable pv) internal   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_static(address payable pv) internal   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_lowlevel_encode(address payable pv) internal   returns (uint256) { Callee_addrp c = new Callee_addrp(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addrp.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_new_arg(address payable pv) internal   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r = c.f2(payable(address(0x1111111111111111111111111111111111111111)), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f2(payable(address(0x1111111111111111111111111111111111111111)), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r = c.f2(payable(address(0x1111111111111111111111111111111111111111)), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addrp.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_addrp c = new Callee_addrp(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addrp.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addrp.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r = c.f(ry); return r; }
}
contract Cctr_0 {
    function _s_retdirect_ext_arg() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg(IC pv) internal   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg_second(IC pv) internal   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f2(IC(address(0x1111111111111111111111111111111111111111)), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try(IC pv) internal   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try_catch_arg(IC pv) internal   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_static(IC pv) internal   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_lowlevel_encode(IC pv) internal   returns (uint256) { Callee_ctr c = new Callee_ctr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ctr.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_new_arg(IC pv) internal   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r = c.f2(IC(address(0x1111111111111111111111111111111111111111)), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f2(IC(address(0x1111111111111111111111111111111111111111)), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r = c.f2(IC(address(0x1111111111111111111111111111111111111111)), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ctr.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_ctr c = new Callee_ctr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ctr.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ctr.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r = c.f(ry); return r; }
}
