//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cb1_0::t_local_ext_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_ext_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_ext_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_ext_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_ext_arg_second => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_ext_arg_second => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_ext_arg_second => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_ext_arg_second => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_ext_try => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_ext_try => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_ext_try => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_ext_try => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_ext_try_catch_arg => 0
//@ run-call: Cb1_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Cb1_0::t_param_ext_try_catch_arg => 0
//@ run-call: Cb1_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Cb1_0::t_local_ext_static => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_ext_static => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_ext_static => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_ext_static => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_ext_lowlevel_encode => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_ext_lowlevel_encode => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_ext_lowlevel_encode => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_ext_lowlevel_encode => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_ext_new_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_ext_new_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_ext_new_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_ext_new_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb4_0::t_local_ext_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_ext_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_ext_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_ext_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_ext_arg_second => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_ext_arg_second => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_ext_arg_second => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_ext_arg_second => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_ext_try => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_ext_try => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_ext_try => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_ext_try => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_ext_try_catch_arg => 0
//@ run-call: Cb4_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Cb4_0::t_param_ext_try_catch_arg => 0
//@ run-call: Cb4_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Cb4_0::t_local_ext_static => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_ext_static => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_ext_static => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_ext_static => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_ext_lowlevel_encode => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_ext_lowlevel_encode => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_ext_lowlevel_encode => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_ext_lowlevel_encode => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_ext_new_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_ext_new_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_ext_new_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_ext_new_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb20_0::t_local_ext_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_ext_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_ext_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_ext_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_ext_arg_second => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_ext_arg_second => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_ext_arg_second => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_ext_arg_second => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_ext_try => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_ext_try => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_ext_try => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_ext_try => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_ext_try_catch_arg => 0
//@ run-call: Cb20_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Cb20_0::t_param_ext_try_catch_arg => 0
//@ run-call: Cb20_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Cb20_0::t_local_ext_static => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_ext_static => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_ext_static => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_ext_static => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_ext_lowlevel_encode => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_ext_lowlevel_encode => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_ext_lowlevel_encode => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_ext_lowlevel_encode => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_ext_new_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_ext_new_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_ext_new_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_ext_new_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880

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

contract Callee_b1 {
    function f(bytes1 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(bytes1 a, bytes1 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(bytes1 v) external pure {
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

contract Callee_b4 {
    function f(bytes4 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(bytes4 a, bytes4 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(bytes4 v) external pure {
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

contract Callee_b20 {
    function f(bytes20 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(bytes20 a, bytes20 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(bytes20 v) external pure {
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

contract Cb1_0 {
    function _s_retdirect_ext_arg() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_arg(bytes1 pv) internal   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_arg_second(bytes1 pv) internal   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f2(bytes1(0xcd), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_try(bytes1 pv) internal   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_try_catch_arg(bytes1 pv) internal   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_static(bytes1 pv) internal   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_lowlevel_encode(bytes1 pv) internal   returns (uint256) { Callee_b1 c = new Callee_b1(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b1.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_new_arg(bytes1 pv) internal   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r = c.f2(bytes1(0xcd), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f2(bytes1(0xcd), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r = c.f2(bytes1(0xcd), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b1.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_b1 c = new Callee_b1(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b1.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b1.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r = c.f(ry); return r; }
}
contract Cb4_0 {
    function _s_retdirect_ext_arg() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_arg(bytes4 pv) internal   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_arg_second(bytes4 pv) internal   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f2(bytes4(0x11223344), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_try(bytes4 pv) internal   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_try_catch_arg(bytes4 pv) internal   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_static(bytes4 pv) internal   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_lowlevel_encode(bytes4 pv) internal   returns (uint256) { Callee_b4 c = new Callee_b4(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b4.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_new_arg(bytes4 pv) internal   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r = c.f2(bytes4(0x11223344), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f2(bytes4(0x11223344), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r = c.f2(bytes4(0x11223344), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b4.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_b4 c = new Callee_b4(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b4.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b4.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r = c.f(ry); return r; }
}
contract Cb20_0 {
    function _s_retdirect_ext_arg() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_arg(bytes20 pv) internal   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_arg_second(bytes20 pv) internal   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f2(bytes20(0x1111111111111111111111111111111111111111), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_try(bytes20 pv) internal   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_try_catch_arg(bytes20 pv) internal   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_static(bytes20 pv) internal   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_lowlevel_encode(bytes20 pv) internal   returns (uint256) { Callee_b20 c = new Callee_b20(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b20.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_new_arg(bytes20 pv) internal   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(pv); return r; }
    function t_local_ext_arg() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r = c.f2(bytes20(0x1111111111111111111111111111111111111111), rx); return r; }
    function t_retdirect_ext_arg_second() external   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f2(bytes20(0x1111111111111111111111111111111111111111), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r = c.f2(bytes20(0x1111111111111111111111111111111111111111), ry); return r; }
    function t_local_ext_try() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b20.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external   returns (uint256) { Callee_b20 c = new Callee_b20(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b20.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b20.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external   returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r = c.f(ry); return r; }
}
