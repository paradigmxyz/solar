//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Caddr_0::t_local_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Caddr_0::t_retdirect_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Caddr_0::t_param_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Caddr_0::t_mem_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Caddr_0::t_local_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Caddr_0::t_retdirect_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Caddr_0::t_param_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Caddr_0::t_mem_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Caddr_0::t_local_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Caddr_0::t_retdirect_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Caddr_0::t_param_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Caddr_0::t_mem_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Caddr_0::t_local_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Caddr_0::t_retdirect_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Caddr_0::t_param_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Caddr_0::t_mem_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Caddr_0::t_local_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Caddr_0::t_retdirect_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Caddr_0::t_param_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Caddr_0::t_mem_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Caddr_0::t_local_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Caddr_0::t_retdirect_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Caddr_0::t_param_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Caddr_0::t_mem_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Caddrp_0::t_local_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Caddrp_0::t_retdirect_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Caddrp_0::t_param_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Caddrp_0::t_mem_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Caddrp_0::t_local_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Caddrp_0::t_retdirect_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Caddrp_0::t_param_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Caddrp_0::t_mem_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Caddrp_0::t_local_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Caddrp_0::t_retdirect_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Caddrp_0::t_param_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Caddrp_0::t_mem_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Caddrp_0::t_local_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Caddrp_0::t_retdirect_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Caddrp_0::t_param_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Caddrp_0::t_mem_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Caddrp_0::t_local_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Caddrp_0::t_retdirect_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Caddrp_0::t_param_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Caddrp_0::t_mem_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Caddrp_0::t_local_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Caddrp_0::t_retdirect_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Caddrp_0::t_param_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Caddrp_0::t_mem_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Cctr_0::t_local_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Cctr_0::t_retdirect_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Cctr_0::t_param_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Cctr_0::t_mem_encode => 57595036398191426918756988898736239661495677068321194020690107645131684302921
//@ run-call: Cctr_0::t_local_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Cctr_0::t_retdirect_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Cctr_0::t_param_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Cctr_0::t_mem_encode_two => 113767051752850486694655484473731178368518334352234759312537124966907740216028
//@ run-call: Cctr_0::t_local_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Cctr_0::t_retdirect_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Cctr_0::t_param_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Cctr_0::t_mem_encode_array => 45392036765026825416151380400308363279889440510633386482327454287958150627609
//@ run-call: Cctr_0::t_local_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Cctr_0::t_retdirect_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Cctr_0::t_param_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Cctr_0::t_mem_encode_struct => 73619411924425386560665428877826545153799062674953099270870737349770588554230
//@ run-call: Cctr_0::t_local_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Cctr_0::t_retdirect_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Cctr_0::t_param_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Cctr_0::t_mem_encode_array_raw_elem => 59816452087255535191145275678570015041060216889650496208486536061212815918328
//@ run-call: Cctr_0::t_local_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Cctr_0::t_retdirect_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Cctr_0::t_param_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281
//@ run-call: Cctr_0::t_mem_encode_struct_raw_field => 64585300483528504342132849195686818194554685533138301778665271221683214174281

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

struct St_addrp {
    uint256 a;
    address payable f;
}

struct St_ctr {
    uint256 a;
    IC f;
}

contract Caddr_0 {
    function _s_retdirect_encode() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode(address pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_two(address pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(address(0x1111111111111111111111111111111111111111), pv, address(0x1111111111111111111111111111111111111111)))); }
    function _s_retdirect_encode_array() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_array(address pv) internal view  returns (uint256) { address[] memory arr3 = new address[](2); arr3[0] = pv; arr3[1] = address(0x1111111111111111111111111111111111111111); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_struct(address pv) internal view  returns (uint256) { St_addr memory s = St_addr(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_array_raw_elem(address pv) internal view  returns (uint256) { address[] memory arr3 = new address[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_struct_raw_field(address pv) internal view  returns (uint256) { St_addr memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encode(address(0x1111111111111111111111111111111111111111), rx, address(0x1111111111111111111111111111111111111111)))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(address(0x1111111111111111111111111111111111111111), _s_retdirect_encode_two(), address(0x1111111111111111111111111111111111111111)))); }
    function t_param_encode_two() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(keccak256(abi.encode(address(0x1111111111111111111111111111111111111111), ry, address(0x1111111111111111111111111111111111111111)))); }
    function t_local_encode_array() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[] memory arr3 = new address[](2); arr3[0] = rx; arr3[1] = address(0x1111111111111111111111111111111111111111); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { address[] memory arr3 = new address[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = address(0x1111111111111111111111111111111111111111); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[] memory arr3 = new address[](2); arr3[0] = ry; arr3[1] = address(0x1111111111111111111111111111111111111111); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addr memory s = St_addr(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_addr memory s = St_addr(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; St_addr memory s = St_addr(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[] memory arr3 = new address[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { address[] memory arr3 = new address[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[] memory arr3 = new address[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addr memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_addr memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; St_addr memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
}
contract Caddrp_0 {
    function _s_retdirect_encode() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode(address payable pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_two(address payable pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(payable(address(0x1111111111111111111111111111111111111111)), pv, payable(address(0x1111111111111111111111111111111111111111))))); }
    function _s_retdirect_encode_array() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_array(address payable pv) internal view  returns (uint256) { address payable[] memory arr3 = new address payable[](2); arr3[0] = pv; arr3[1] = payable(address(0x1111111111111111111111111111111111111111)); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_struct(address payable pv) internal view  returns (uint256) { St_addrp memory s = St_addrp(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_array_raw_elem(address payable pv) internal view  returns (uint256) { address payable[] memory arr3 = new address payable[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_struct_raw_field(address payable pv) internal view  returns (uint256) { St_addrp memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encode(payable(address(0x1111111111111111111111111111111111111111)), rx, payable(address(0x1111111111111111111111111111111111111111))))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(payable(address(0x1111111111111111111111111111111111111111)), _s_retdirect_encode_two(), payable(address(0x1111111111111111111111111111111111111111))))); }
    function t_param_encode_two() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(keccak256(abi.encode(payable(address(0x1111111111111111111111111111111111111111)), ry, payable(address(0x1111111111111111111111111111111111111111))))); }
    function t_local_encode_array() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[] memory arr3 = new address payable[](2); arr3[0] = rx; arr3[1] = payable(address(0x1111111111111111111111111111111111111111)); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { address payable[] memory arr3 = new address payable[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = payable(address(0x1111111111111111111111111111111111111111)); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[] memory arr3 = new address payable[](2); arr3[0] = ry; arr3[1] = payable(address(0x1111111111111111111111111111111111111111)); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addrp memory s = St_addrp(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_addrp memory s = St_addrp(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; St_addrp memory s = St_addrp(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[] memory arr3 = new address payable[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { address payable[] memory arr3 = new address payable[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[] memory arr3 = new address payable[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addrp memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_addrp memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; St_addrp memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
}
contract Cctr_0 {
    function _s_retdirect_encode() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode(IC pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_two(IC pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(IC(address(0x1111111111111111111111111111111111111111)), pv, IC(address(0x1111111111111111111111111111111111111111))))); }
    function _s_retdirect_encode_array() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_array(IC pv) internal view  returns (uint256) { IC[] memory arr3 = new IC[](2); arr3[0] = pv; arr3[1] = IC(address(0x1111111111111111111111111111111111111111)); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_struct(IC pv) internal view  returns (uint256) { St_ctr memory s = St_ctr(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_array_raw_elem(IC pv) internal view  returns (uint256) { IC[] memory arr3 = new IC[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_struct_raw_field(IC pv) internal view  returns (uint256) { St_ctr memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encode(IC(address(0x1111111111111111111111111111111111111111)), rx, IC(address(0x1111111111111111111111111111111111111111))))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(IC(address(0x1111111111111111111111111111111111111111)), _s_retdirect_encode_two(), IC(address(0x1111111111111111111111111111111111111111))))); }
    function t_param_encode_two() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(keccak256(abi.encode(IC(address(0x1111111111111111111111111111111111111111)), ry, IC(address(0x1111111111111111111111111111111111111111))))); }
    function t_local_encode_array() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[] memory arr3 = new IC[](2); arr3[0] = rx; arr3[1] = IC(address(0x1111111111111111111111111111111111111111)); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { IC[] memory arr3 = new IC[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = IC(address(0x1111111111111111111111111111111111111111)); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[] memory arr3 = new IC[](2); arr3[0] = ry; arr3[1] = IC(address(0x1111111111111111111111111111111111111111)); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_ctr memory s = St_ctr(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_ctr memory s = St_ctr(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; St_ctr memory s = St_ctr(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[] memory arr3 = new IC[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { IC[] memory arr3 = new IC[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[] memory arr3 = new IC[](1); assembly { mstore(add(arr3, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_ctr memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_ctr memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; St_ctr memory s; assembly { mstore(add(s, 32), 0xabcd1234567890123456789012345678901234567890) } return uint256(keccak256(abi.encode(s))); }
}
