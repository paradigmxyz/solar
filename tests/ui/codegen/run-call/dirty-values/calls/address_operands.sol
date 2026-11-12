//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740 --allow=9207 --allow=5878
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Caddr_0::t_local_addr_balance; value=7 => 7
//@ run-call: Caddr_0::t_retdirect_addr_balance; value=7 => 7
//@ run-call: Caddr_0::t_param_addr_balance; value=7 => 7
//@ run-call: Caddr_0::t_mem_addr_balance; value=7 => 7
//@ run-call: Caddr_0::t_local_addr_code_len => 1
//@ run-call: Caddr_0::t_retdirect_addr_code_len => 1
//@ run-call: Caddr_0::t_param_addr_code_len => 1
//@ run-call: Caddr_0::t_mem_addr_code_len => 1
//@ run-call: Caddr_0::t_local_addr_codehash => 1
//@ run-call: Caddr_0::t_retdirect_addr_codehash => 1
//@ run-call: Caddr_0::t_param_addr_codehash => 1
//@ run-call: Caddr_0::t_mem_addr_codehash => 1
//@ run-call: Caddr_0::t_local_addr_call => 77
//@ run-call: Caddr_0::t_retdirect_addr_call => 77
//@ run-call: Caddr_0::t_param_addr_call => 77
//@ run-call: Caddr_0::t_mem_addr_call => 77
//@ run-call: Caddr_0::t_local_addr_staticcall => 77
//@ run-call: Caddr_0::t_retdirect_addr_staticcall => 77
//@ run-call: Caddr_0::t_param_addr_staticcall => 77
//@ run-call: Caddr_0::t_mem_addr_staticcall => 77
//@ run-call: Caddr_0::t_local_addr_delegatecall => 77
//@ run-call: Caddr_0::t_retdirect_addr_delegatecall => 77
//@ run-call: Caddr_0::t_param_addr_delegatecall => 77
//@ run-call: Caddr_0::t_mem_addr_delegatecall => 77
//@ run-call: Caddr_0::t_local_addr_ext_call => 1
//@ run-call: Caddr_0::t_retdirect_addr_ext_call => 1
//@ run-call: Caddr_0::t_param_addr_ext_call => 1
//@ run-call: Caddr_0::t_mem_addr_ext_call => 1
//@ run-call: Caddrp_0::t_local_addr_balance; value=7 => 7
//@ run-call: Caddrp_0::t_retdirect_addr_balance; value=7 => 7
//@ run-call: Caddrp_0::t_param_addr_balance; value=7 => 7
//@ run-call: Caddrp_0::t_mem_addr_balance; value=7 => 7
//@ run-call: Caddrp_0::t_local_addr_code_len => 1
//@ run-call: Caddrp_0::t_retdirect_addr_code_len => 1
//@ run-call: Caddrp_0::t_param_addr_code_len => 1
//@ run-call: Caddrp_0::t_mem_addr_code_len => 1
//@ run-call: Caddrp_0::t_local_addr_codehash => 1
//@ run-call: Caddrp_0::t_retdirect_addr_codehash => 1
//@ run-call: Caddrp_0::t_param_addr_codehash => 1
//@ run-call: Caddrp_0::t_mem_addr_codehash => 1
//@ run-call: Caddrp_0::t_local_addr_call => 77
//@ run-call: Caddrp_0::t_retdirect_addr_call => 77
//@ run-call: Caddrp_0::t_param_addr_call => 77
//@ run-call: Caddrp_0::t_mem_addr_call => 77
//@ run-call: Caddrp_0::t_local_addr_staticcall => 77
//@ run-call: Caddrp_0::t_retdirect_addr_staticcall => 77
//@ run-call: Caddrp_0::t_param_addr_staticcall => 77
//@ run-call: Caddrp_0::t_mem_addr_staticcall => 77
//@ run-call: Caddrp_0::t_local_addr_delegatecall => 77
//@ run-call: Caddrp_0::t_retdirect_addr_delegatecall => 77
//@ run-call: Caddrp_0::t_param_addr_delegatecall => 77
//@ run-call: Caddrp_0::t_mem_addr_delegatecall => 77
//@ run-call: Caddrp_0::t_local_addr_ext_call => 1
//@ run-call: Caddrp_0::t_retdirect_addr_ext_call => 1
//@ run-call: Caddrp_0::t_param_addr_ext_call => 1
//@ run-call: Caddrp_0::t_mem_addr_ext_call => 1
//@ run-call: Caddrp_0::t_local_addr_send; value=7 => 1
//@ run-call: Caddrp_0::t_retdirect_addr_send; value=7 => 1
//@ run-call: Caddrp_0::t_param_addr_send; value=7 => 1
//@ run-call: Caddrp_0::t_mem_addr_send; value=7 => 1
//@ run-call: Caddrp_0::t_local_addr_transfer; value=7 => 1
//@ run-call: Caddrp_0::t_retdirect_addr_transfer; value=7 => 1
//@ run-call: Caddrp_0::t_param_addr_transfer; value=7 => 1
//@ run-call: Caddrp_0::t_mem_addr_transfer; value=7 => 1
//@ run-call: Cctr_0::t_local_addr_balance; value=7 => 7
//@ run-call: Cctr_0::t_retdirect_addr_balance; value=7 => 7
//@ run-call: Cctr_0::t_param_addr_balance; value=7 => 7
//@ run-call: Cctr_0::t_mem_addr_balance; value=7 => 7
//@ run-call: Cctr_0::t_local_addr_code_len => 1
//@ run-call: Cctr_0::t_retdirect_addr_code_len => 1
//@ run-call: Cctr_0::t_param_addr_code_len => 1
//@ run-call: Cctr_0::t_mem_addr_code_len => 1
//@ run-call: Cctr_0::t_local_addr_codehash => 1
//@ run-call: Cctr_0::t_retdirect_addr_codehash => 1
//@ run-call: Cctr_0::t_param_addr_codehash => 1
//@ run-call: Cctr_0::t_mem_addr_codehash => 1
//@ run-call: Cctr_0::t_local_addr_call => 77
//@ run-call: Cctr_0::t_retdirect_addr_call => 77
//@ run-call: Cctr_0::t_param_addr_call => 77
//@ run-call: Cctr_0::t_mem_addr_call => 77
//@ run-call: Cctr_0::t_local_addr_staticcall => 77
//@ run-call: Cctr_0::t_retdirect_addr_staticcall => 77
//@ run-call: Cctr_0::t_param_addr_staticcall => 77
//@ run-call: Cctr_0::t_mem_addr_staticcall => 77
//@ run-call: Cctr_0::t_local_addr_delegatecall => 77
//@ run-call: Cctr_0::t_retdirect_addr_delegatecall => 77
//@ run-call: Cctr_0::t_param_addr_delegatecall => 77
//@ run-call: Cctr_0::t_mem_addr_delegatecall => 77
//@ run-call: Cctr_0::t_local_addr_ext_call => 1
//@ run-call: Cctr_0::t_retdirect_addr_ext_call => 1
//@ run-call: Cctr_0::t_param_addr_ext_call => 1
//@ run-call: Cctr_0::t_mem_addr_ext_call => 1
//@ run-call: Cctr_0::t_local_ctr_ext_call => 1
//@ run-call: Cctr_0::t_retdirect_ctr_ext_call => 1
//@ run-call: Cctr_0::t_param_ctr_ext_call => 1
//@ run-call: Cctr_0::t_mem_ctr_ext_call => 1

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

contract Caddr_0 {
    function ping() external pure returns (uint256) { return 77; }
    receive() external payable {}
    function f() external {}
    function _s_retdirect_addr_balance() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_balance(address pv) internal   returns (uint256) { return address(pv).balance; }
    function _s_retdirect_addr_code_len() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_code_len(address pv) internal view  returns (uint256) { return address(pv).code.length == address(this).code.length ? 1 : 0; }
    function _s_retdirect_addr_codehash() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_codehash(address pv) internal view  returns (uint256) { return address(pv).codehash == address(this).codehash ? 1 : 0; }
    function _s_retdirect_addr_call() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_call(address pv) internal   returns (uint256) { (bool ok, bytes memory out) = address(pv).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_staticcall() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_staticcall(address pv) internal view  returns (uint256) { (bool ok, bytes memory out) = address(pv).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_delegatecall() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_delegatecall(address pv) internal   returns (uint256) { (bool ok, bytes memory out) = address(pv).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_ext_call() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_ext_call(address pv) internal   returns (uint256) { IC(address(pv)).f(); return 1; }
    function t_local_addr_balance() external payable  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).balance; }
    function t_retdirect_addr_balance() external payable  returns (uint256) { return address(_s_retdirect_addr_balance()).balance; }
    function t_param_addr_balance() external payable  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_balance(rx); }
    function t_mem_addr_balance() external payable  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; return address(ry).balance; }
    function t_local_addr_code_len() external view  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).code.length == address(this).code.length ? 1 : 0; }
    function t_retdirect_addr_code_len() external view  returns (uint256) { return address(_s_retdirect_addr_code_len()).code.length == address(this).code.length ? 1 : 0; }
    function t_param_addr_code_len() external view  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_code_len(rx); }
    function t_mem_addr_code_len() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; return address(ry).code.length == address(this).code.length ? 1 : 0; }
    function t_local_addr_codehash() external view  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).codehash == address(this).codehash ? 1 : 0; }
    function t_retdirect_addr_codehash() external view  returns (uint256) { return address(_s_retdirect_addr_codehash()).codehash == address(this).codehash ? 1 : 0; }
    function t_param_addr_codehash() external view  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_codehash(rx); }
    function t_mem_addr_codehash() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; return address(ry).codehash == address(this).codehash ? 1 : 0; }
    function t_local_addr_call() external   returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_call() external   returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_call()).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_call() external   returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_call(rx); }
    function t_mem_addr_call() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; (bool ok, bytes memory out) = address(ry).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_staticcall() external view  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_staticcall() external view  returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_staticcall()).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_staticcall() external view  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_staticcall(rx); }
    function t_mem_addr_staticcall() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; (bool ok, bytes memory out) = address(ry).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_delegatecall() external   returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_delegatecall() external   returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_delegatecall()).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_delegatecall() external   returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_delegatecall(rx); }
    function t_mem_addr_delegatecall() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; (bool ok, bytes memory out) = address(ry).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_ext_call() external   returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } IC(address(rx)).f(); return 1; }
    function t_retdirect_addr_ext_call() external   returns (uint256) { IC(address(_s_retdirect_addr_ext_call())).f(); return 1; }
    function t_param_addr_ext_call() external   returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_ext_call(rx); }
    function t_mem_addr_ext_call() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; IC(address(ry)).f(); return 1; }
}
contract Caddrp_0 {
    function ping() external pure returns (uint256) { return 77; }
    receive() external payable {}
    function f() external {}
    function _s_retdirect_addr_balance() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_balance(address payable pv) internal   returns (uint256) { return address(pv).balance; }
    function _s_retdirect_addr_code_len() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_code_len(address payable pv) internal view  returns (uint256) { return address(pv).code.length == address(this).code.length ? 1 : 0; }
    function _s_retdirect_addr_codehash() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_codehash(address payable pv) internal view  returns (uint256) { return address(pv).codehash == address(this).codehash ? 1 : 0; }
    function _s_retdirect_addr_call() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_call(address payable pv) internal   returns (uint256) { (bool ok, bytes memory out) = address(pv).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_staticcall() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_staticcall(address payable pv) internal view  returns (uint256) { (bool ok, bytes memory out) = address(pv).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_delegatecall() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_delegatecall(address payable pv) internal   returns (uint256) { (bool ok, bytes memory out) = address(pv).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_ext_call() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_ext_call(address payable pv) internal   returns (uint256) { IC(address(pv)).f(); return 1; }
    function _s_retdirect_addr_send() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_send(address payable pv) internal   returns (uint256) { bool ok = pv.send(1); return ok ? 1 : 0; }
    function _s_retdirect_addr_transfer() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_transfer(address payable pv) internal   returns (uint256) { pv.transfer(1); return 1; }
    function t_local_addr_balance() external payable  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).balance; }
    function t_retdirect_addr_balance() external payable  returns (uint256) { return address(_s_retdirect_addr_balance()).balance; }
    function t_param_addr_balance() external payable  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_balance(rx); }
    function t_mem_addr_balance() external payable  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; return address(ry).balance; }
    function t_local_addr_code_len() external view  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).code.length == address(this).code.length ? 1 : 0; }
    function t_retdirect_addr_code_len() external view  returns (uint256) { return address(_s_retdirect_addr_code_len()).code.length == address(this).code.length ? 1 : 0; }
    function t_param_addr_code_len() external view  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_code_len(rx); }
    function t_mem_addr_code_len() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; return address(ry).code.length == address(this).code.length ? 1 : 0; }
    function t_local_addr_codehash() external view  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).codehash == address(this).codehash ? 1 : 0; }
    function t_retdirect_addr_codehash() external view  returns (uint256) { return address(_s_retdirect_addr_codehash()).codehash == address(this).codehash ? 1 : 0; }
    function t_param_addr_codehash() external view  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_codehash(rx); }
    function t_mem_addr_codehash() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; return address(ry).codehash == address(this).codehash ? 1 : 0; }
    function t_local_addr_call() external   returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_call() external   returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_call()).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_call() external   returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_call(rx); }
    function t_mem_addr_call() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; (bool ok, bytes memory out) = address(ry).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_staticcall() external view  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_staticcall() external view  returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_staticcall()).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_staticcall() external view  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_staticcall(rx); }
    function t_mem_addr_staticcall() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; (bool ok, bytes memory out) = address(ry).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_delegatecall() external   returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_delegatecall() external   returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_delegatecall()).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_delegatecall() external   returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_delegatecall(rx); }
    function t_mem_addr_delegatecall() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; (bool ok, bytes memory out) = address(ry).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_ext_call() external   returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } IC(address(rx)).f(); return 1; }
    function t_retdirect_addr_ext_call() external   returns (uint256) { IC(address(_s_retdirect_addr_ext_call())).f(); return 1; }
    function t_param_addr_ext_call() external   returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_ext_call(rx); }
    function t_mem_addr_ext_call() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; IC(address(ry)).f(); return 1; }
    function t_local_addr_send() external payable  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } bool ok = rx.send(1); return ok ? 1 : 0; }
    function t_retdirect_addr_send() external payable  returns (uint256) { bool ok = _s_retdirect_addr_send().send(1); return ok ? 1 : 0; }
    function t_param_addr_send() external payable  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_send(rx); }
    function t_mem_addr_send() external payable  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; bool ok = ry.send(1); return ok ? 1 : 0; }
    function t_local_addr_transfer() external payable  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } rx.transfer(1); return 1; }
    function t_retdirect_addr_transfer() external payable  returns (uint256) { _s_retdirect_addr_transfer().transfer(1); return 1; }
    function t_param_addr_transfer() external payable  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_transfer(rx); }
    function t_mem_addr_transfer() external payable  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; ry.transfer(1); return 1; }
}
contract Cctr_0 {
    function ping() external pure returns (uint256) { return 77; }
    receive() external payable {}
    function f() external {}
    function _s_retdirect_addr_balance() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_balance(IC pv) internal   returns (uint256) { return address(pv).balance; }
    function _s_retdirect_addr_code_len() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_code_len(IC pv) internal view  returns (uint256) { return address(pv).code.length == address(this).code.length ? 1 : 0; }
    function _s_retdirect_addr_codehash() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_codehash(IC pv) internal view  returns (uint256) { return address(pv).codehash == address(this).codehash ? 1 : 0; }
    function _s_retdirect_addr_call() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_call(IC pv) internal   returns (uint256) { (bool ok, bytes memory out) = address(pv).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_staticcall() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_staticcall(IC pv) internal view  returns (uint256) { (bool ok, bytes memory out) = address(pv).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_delegatecall() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_delegatecall(IC pv) internal   returns (uint256) { (bool ok, bytes memory out) = address(pv).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_ext_call() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_ext_call(IC pv) internal   returns (uint256) { IC(address(pv)).f(); return 1; }
    function _s_retdirect_ctr_ext_call() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_ctr_ext_call(IC pv) internal   returns (uint256) { pv.f(); return 1; }
    function t_local_addr_balance() external payable  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).balance; }
    function t_retdirect_addr_balance() external payable  returns (uint256) { return address(_s_retdirect_addr_balance()).balance; }
    function t_param_addr_balance() external payable  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_balance(rx); }
    function t_mem_addr_balance() external payable  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; return address(ry).balance; }
    function t_local_addr_code_len() external view  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).code.length == address(this).code.length ? 1 : 0; }
    function t_retdirect_addr_code_len() external view  returns (uint256) { return address(_s_retdirect_addr_code_len()).code.length == address(this).code.length ? 1 : 0; }
    function t_param_addr_code_len() external view  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_code_len(rx); }
    function t_mem_addr_code_len() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; return address(ry).code.length == address(this).code.length ? 1 : 0; }
    function t_local_addr_codehash() external view  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).codehash == address(this).codehash ? 1 : 0; }
    function t_retdirect_addr_codehash() external view  returns (uint256) { return address(_s_retdirect_addr_codehash()).codehash == address(this).codehash ? 1 : 0; }
    function t_param_addr_codehash() external view  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_codehash(rx); }
    function t_mem_addr_codehash() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; return address(ry).codehash == address(this).codehash ? 1 : 0; }
    function t_local_addr_call() external   returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_call() external   returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_call()).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_call() external   returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_call(rx); }
    function t_mem_addr_call() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; (bool ok, bytes memory out) = address(ry).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_staticcall() external view  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_staticcall() external view  returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_staticcall()).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_staticcall() external view  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_staticcall(rx); }
    function t_mem_addr_staticcall() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; (bool ok, bytes memory out) = address(ry).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_delegatecall() external   returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_delegatecall() external   returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_delegatecall()).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_delegatecall() external   returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_delegatecall(rx); }
    function t_mem_addr_delegatecall() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; (bool ok, bytes memory out) = address(ry).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_ext_call() external   returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } IC(address(rx)).f(); return 1; }
    function t_retdirect_addr_ext_call() external   returns (uint256) { IC(address(_s_retdirect_addr_ext_call())).f(); return 1; }
    function t_param_addr_ext_call() external   returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_ext_call(rx); }
    function t_mem_addr_ext_call() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; IC(address(ry)).f(); return 1; }
    function t_local_ctr_ext_call() external   returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } rx.f(); return 1; }
    function t_retdirect_ctr_ext_call() external   returns (uint256) { _s_retdirect_ctr_ext_call().f(); return 1; }
    function t_param_ctr_ext_call() external   returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_ctr_ext_call(rx); }
    function t_mem_ctr_ext_call() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; ry.f(); return 1; }
}
