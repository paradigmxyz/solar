//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_encode_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_retdirect_encode_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_param_encode_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_mem_encode_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cu8_0::t_local_encode_packed_three => 30324008857266915950417497675321363767344981613584688184295061256773276698108
//@ run-call: Cu8_0::t_retdirect_encode_packed_three => 30324008857266915950417497675321363767344981613584688184295061256773276698108
//@ run-call: Cu8_0::t_param_encode_packed_three => 30324008857266915950417497675321363767344981613584688184295061256773276698108
//@ run-call: Cu8_0::t_mem_encode_packed_three => 30324008857266915950417497675321363767344981613584688184295061256773276698108
//@ run-call: Cu64_0::t_local_encode_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_retdirect_encode_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_param_encode_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_mem_encode_packed => 108380341783020705080973623281021274336515760181348251453162873399778428344531
//@ run-call: Cu64_0::t_local_encode_packed_three => 45471594996121460122020180458676601468841052007616086817114812943071887529104
//@ run-call: Cu64_0::t_retdirect_encode_packed_three => 45471594996121460122020180458676601468841052007616086817114812943071887529104
//@ run-call: Cu64_0::t_param_encode_packed_three => 45471594996121460122020180458676601468841052007616086817114812943071887529104
//@ run-call: Cu64_0::t_mem_encode_packed_three => 45471594996121460122020180458676601468841052007616086817114812943071887529104
//@ run-call: Ci8_0::t_local_encode_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_retdirect_encode_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_param_encode_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_mem_encode_packed => 94189660818352312039012027670723710764638516952376447377847877200768457404736
//@ run-call: Ci8_0::t_local_encode_packed_three => 110615947558695832368147334914960589055041034653641056506295656987091172204911
//@ run-call: Ci8_0::t_retdirect_encode_packed_three => 110615947558695832368147334914960589055041034653641056506295656987091172204911
//@ run-call: Ci8_0::t_param_encode_packed_three => 110615947558695832368147334914960589055041034653641056506295656987091172204911
//@ run-call: Ci8_0::t_mem_encode_packed_three => 110615947558695832368147334914960589055041034653641056506295656987091172204911
//@ run-call: Ci24_0::t_local_encode_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_retdirect_encode_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_param_encode_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_mem_encode_packed => 79138035952336153262678106899457937836297017945557076829363943559903750223177
//@ run-call: Ci24_0::t_local_encode_packed_three => 85262079195094858692925155617124304514092386766850840212225628392811653787407
//@ run-call: Ci24_0::t_retdirect_encode_packed_three => 85262079195094858692925155617124304514092386766850840212225628392811653787407
//@ run-call: Ci24_0::t_param_encode_packed_three => 85262079195094858692925155617124304514092386766850840212225628392811653787407
//@ run-call: Ci24_0::t_mem_encode_packed_three => 85262079195094858692925155617124304514092386766850840212225628392811653787407
//@ run-call: Caddr_0::t_local_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_retdirect_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_param_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_mem_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddr_0::t_local_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Caddr_0::t_retdirect_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Caddr_0::t_param_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Caddr_0::t_mem_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Caddrp_0::t_local_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_retdirect_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_param_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_mem_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Caddrp_0::t_local_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Caddrp_0::t_retdirect_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Caddrp_0::t_param_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Caddrp_0::t_mem_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Cctr_0::t_local_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_retdirect_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_param_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_mem_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cctr_0::t_local_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Cctr_0::t_retdirect_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Cctr_0::t_param_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Cctr_0::t_mem_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Cbool_0::t_local_encode_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_retdirect_encode_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_param_encode_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_mem_encode_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_local_encode_packed_three => 103294036228779678485822656133922965336661686526231159321739468925376744814047
//@ run-call: Cbool_0::t_retdirect_encode_packed_three => 103294036228779678485822656133922965336661686526231159321739468925376744814047
//@ run-call: Cbool_0::t_param_encode_packed_three => 103294036228779678485822656133922965336661686526231159321739468925376744814047
//@ run-call: Cbool_0::t_mem_encode_packed_three => 103294036228779678485822656133922965336661686526231159321739468925376744814047
//@ run-call: Cb1_0::t_local_encode_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_retdirect_encode_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_param_encode_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_mem_encode_packed => 31915950958967079880369911692532962067266660999621450220525506028933044220024
//@ run-call: Cb1_0::t_local_encode_packed_three => 74543062364992850223212913648461878382506726546650975767766567428223087697030
//@ run-call: Cb1_0::t_retdirect_encode_packed_three => 74543062364992850223212913648461878382506726546650975767766567428223087697030
//@ run-call: Cb1_0::t_param_encode_packed_three => 74543062364992850223212913648461878382506726546650975767766567428223087697030
//@ run-call: Cb1_0::t_mem_encode_packed_three => 74543062364992850223212913648461878382506726546650975767766567428223087697030
//@ run-call: Cb4_0::t_local_encode_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_retdirect_encode_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_param_encode_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_mem_encode_packed => 96337875205586089318242636794245924068320436284154789200033428706670420166385
//@ run-call: Cb4_0::t_local_encode_packed_three => 105345525819910413689359848256100160544340105494604835063980424270734954260225
//@ run-call: Cb4_0::t_retdirect_encode_packed_three => 105345525819910413689359848256100160544340105494604835063980424270734954260225
//@ run-call: Cb4_0::t_param_encode_packed_three => 105345525819910413689359848256100160544340105494604835063980424270734954260225
//@ run-call: Cb4_0::t_mem_encode_packed_three => 105345525819910413689359848256100160544340105494604835063980424270734954260225
//@ run-call: Cb20_0::t_local_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_retdirect_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_param_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_mem_encode_packed => 82588768485447195675861161909360185705996473455108110011558205074549672331103
//@ run-call: Cb20_0::t_local_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Cb20_0::t_retdirect_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Cb20_0::t_param_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call: Cb20_0::t_mem_encode_packed_three => 80652087680554370337390942637177048224253948740398913740146519239271111766515
//@ run-call-fail: Cen_0::t_local_encode_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_encode_packed_three => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode_packed_three => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode_packed_three => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode_packed_three => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_encode_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_retdirect_encode_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_param_encode_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_mem_encode_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_local_encode_packed_three => 30324008857266915950417497675321363767344981613584688184295061256773276698108
//@ run-call: Cud_0::t_retdirect_encode_packed_three => 30324008857266915950417497675321363767344981613584688184295061256773276698108
//@ run-call: Cud_0::t_param_encode_packed_three => 30324008857266915950417497675321363767344981613584688184295061256773276698108
//@ run-call: Cud_0::t_mem_encode_packed_three => 30324008857266915950417497675321363767344981613584688184295061256773276698108

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

contract Cu8_0 {
    function _s_retdirect_encode_packed() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_packed(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_packed_three(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(uint8(3), pv, uint8(3)))); }
    function t_local_encode_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodePacked(uint8(3), rx, uint8(3)))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(uint8(3), _s_retdirect_encode_packed_three(), uint8(3)))); }
    function t_param_encode_packed_three() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encodePacked(uint8(3), ry, uint8(3)))); }
}
contract Cu64_0 {
    function _s_retdirect_encode_packed() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode_packed(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode_packed_three(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(uint64(3), pv, uint64(3)))); }
    function t_local_encode_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encodePacked(uint64(3), rx, uint64(3)))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(uint64(3), _s_retdirect_encode_packed_three(), uint64(3)))); }
    function t_param_encode_packed_three() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encodePacked(uint64(3), ry, uint64(3)))); }
}
contract Ci8_0 {
    function _s_retdirect_encode_packed() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode_packed(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode_packed_three(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(int8(-2), pv, int8(-2)))); }
    function t_local_encode_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encodePacked(int8(-2), rx, int8(-2)))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(int8(-2), _s_retdirect_encode_packed_three(), int8(-2)))); }
    function t_param_encode_packed_three() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encodePacked(int8(-2), ry, int8(-2)))); }
}
contract Ci24_0 {
    function _s_retdirect_encode_packed() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode_packed(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode_packed_three(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(int24(-2), pv, int24(-2)))); }
    function t_local_encode_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encodePacked(int24(-2), rx, int24(-2)))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(int24(-2), _s_retdirect_encode_packed_three(), int24(-2)))); }
    function t_param_encode_packed_three() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encodePacked(int24(-2), ry, int24(-2)))); }
}
contract Caddr_0 {
    function _s_retdirect_encode_packed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_packed(address pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_packed_three(address pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), pv, address(0x1111111111111111111111111111111111111111)))); }
    function t_local_encode_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), rx, address(0x1111111111111111111111111111111111111111)))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), _s_retdirect_encode_packed_three(), address(0x1111111111111111111111111111111111111111)))); }
    function t_param_encode_packed_three() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(keccak256(abi.encodePacked(address(0x1111111111111111111111111111111111111111), ry, address(0x1111111111111111111111111111111111111111)))); }
}
contract Caddrp_0 {
    function _s_retdirect_encode_packed() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_packed(address payable pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_packed_three(address payable pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), pv, payable(address(0x1111111111111111111111111111111111111111))))); }
    function t_local_encode_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), rx, payable(address(0x1111111111111111111111111111111111111111))))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), _s_retdirect_encode_packed_three(), payable(address(0x1111111111111111111111111111111111111111))))); }
    function t_param_encode_packed_three() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(keccak256(abi.encodePacked(payable(address(0x1111111111111111111111111111111111111111)), ry, payable(address(0x1111111111111111111111111111111111111111))))); }
}
contract Cctr_0 {
    function _s_retdirect_encode_packed() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_packed(IC pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_packed_three(IC pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), pv, IC(address(0x1111111111111111111111111111111111111111))))); }
    function t_local_encode_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), rx, IC(address(0x1111111111111111111111111111111111111111))))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), _s_retdirect_encode_packed_three(), IC(address(0x1111111111111111111111111111111111111111))))); }
    function t_param_encode_packed_three() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(keccak256(abi.encodePacked(IC(address(0x1111111111111111111111111111111111111111)), ry, IC(address(0x1111111111111111111111111111111111111111))))); }
}
contract Cbool_0 {
    function _s_retdirect_encode_packed() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode_packed(bool pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode_packed_three(bool pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(false, pv, false))); }
    function t_local_encode_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(keccak256(abi.encodePacked(false, rx, false))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(false, _s_retdirect_encode_packed_three(), false))); }
    function t_param_encode_packed_three() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(keccak256(abi.encodePacked(false, ry, false))); }
}
contract Cb1_0 {
    function _s_retdirect_encode_packed() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode_packed(bytes1 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode_packed_three(bytes1 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes1(0xcd), pv, bytes1(0xcd)))); }
    function t_local_encode_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(keccak256(abi.encodePacked(bytes1(0xcd), rx, bytes1(0xcd)))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes1(0xcd), _s_retdirect_encode_packed_three(), bytes1(0xcd)))); }
    function t_param_encode_packed_three() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(keccak256(abi.encodePacked(bytes1(0xcd), ry, bytes1(0xcd)))); }
}
contract Cb4_0 {
    function _s_retdirect_encode_packed() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode_packed(bytes4 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode_packed_three(bytes4 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), pv, bytes4(0x11223344)))); }
    function t_local_encode_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), rx, bytes4(0x11223344)))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), _s_retdirect_encode_packed_three(), bytes4(0x11223344)))); }
    function t_param_encode_packed_three() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(keccak256(abi.encodePacked(bytes4(0x11223344), ry, bytes4(0x11223344)))); }
}
contract Cb20_0 {
    function _s_retdirect_encode_packed() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode_packed(bytes20 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode_packed_three(bytes20 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), pv, bytes20(0x1111111111111111111111111111111111111111)))); }
    function t_local_encode_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), rx, bytes20(0x1111111111111111111111111111111111111111)))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), _s_retdirect_encode_packed_three(), bytes20(0x1111111111111111111111111111111111111111)))); }
    function t_param_encode_packed_three() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(keccak256(abi.encodePacked(bytes20(0x1111111111111111111111111111111111111111), ry, bytes20(0x1111111111111111111111111111111111111111)))); }
}
contract Cen_0 {
    function _s_retdirect_encode_packed() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode_packed(E pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode_packed_three(E pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(E.A, pv, E.A))); }
    function t_local_encode_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(keccak256(abi.encodePacked(E.A, rx, E.A))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(E.A, _s_retdirect_encode_packed_three(), E.A))); }
    function t_param_encode_packed_three() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(keccak256(abi.encodePacked(E.A, ry, E.A))); }
}
contract Cud_0 {
    function _s_retdirect_encode_packed() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_packed(U pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    function _s_retdirect_encode_packed_three() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_packed_three(U pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(U.wrap(3), pv, U.wrap(3)))); }
    function t_local_encode_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_encode_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_encode_packed()))); }
    function t_param_encode_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode_packed(rx); }
    function t_mem_encode_packed() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_encode_packed_three() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodePacked(U.wrap(3), rx, U.wrap(3)))); }
    function t_retdirect_encode_packed_three() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(U.wrap(3), _s_retdirect_encode_packed_three(), U.wrap(3)))); }
    function t_param_encode_packed_three() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode_packed_three(rx); }
    function t_mem_encode_packed_three() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(keccak256(abi.encodePacked(U.wrap(3), ry, U.wrap(3)))); }
}
