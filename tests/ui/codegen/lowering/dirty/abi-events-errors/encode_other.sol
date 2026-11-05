//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cbool_0::t_local_encode => 80084422859880547211683076133703299733277748156566366325829078699459944778998
//@ run-call: Cbool_0::t_retdirect_encode => 80084422859880547211683076133703299733277748156566366325829078699459944778998
//@ run-call: Cbool_0::t_param_encode => 80084422859880547211683076133703299733277748156566366325829078699459944778998
//@ run-call: Cbool_0::t_mem_encode => 80084422859880547211683076133703299733277748156566366325829078699459944778998
//@ run-call: Cbool_0::t_local_encode_two => 33502794087021486631736485175675607482492527805619155636805170928430967799127
//@ run-call: Cbool_0::t_retdirect_encode_two => 33502794087021486631736485175675607482492527805619155636805170928430967799127
//@ run-call: Cbool_0::t_param_encode_two => 33502794087021486631736485175675607482492527805619155636805170928430967799127
//@ run-call: Cbool_0::t_mem_encode_two => 33502794087021486631736485175675607482492527805619155636805170928430967799127
//@ run-call: Cbool_0::t_local_encode_array => 44158903340759405471142274210976052822829900070509570303958867093565129703096
//@ run-call: Cbool_0::t_retdirect_encode_array => 44158903340759405471142274210976052822829900070509570303958867093565129703096
//@ run-call: Cbool_0::t_param_encode_array => 44158903340759405471142274210976052822829900070509570303958867093565129703096
//@ run-call: Cbool_0::t_mem_encode_array => 44158903340759405471142274210976052822829900070509570303958867093565129703096
//@ run-call: Cbool_0::t_local_encode_struct => 92458281274488595289803937127152923398167637295201432141969818930235769911599
//@ run-call: Cbool_0::t_retdirect_encode_struct => 92458281274488595289803937127152923398167637295201432141969818930235769911599
//@ run-call: Cbool_0::t_param_encode_struct => 92458281274488595289803937127152923398167637295201432141969818930235769911599
//@ run-call: Cbool_0::t_mem_encode_struct => 92458281274488595289803937127152923398167637295201432141969818930235769911599
//@ run-call: Cbool_0::t_local_encode_array_raw_elem => 13789110210920521593631520508978233657055078076298943265723336393165330220949
//@ run-call: Cbool_0::t_retdirect_encode_array_raw_elem => 13789110210920521593631520508978233657055078076298943265723336393165330220949
//@ run-call: Cbool_0::t_param_encode_array_raw_elem => 13789110210920521593631520508978233657055078076298943265723336393165330220949
//@ run-call: Cbool_0::t_mem_encode_array_raw_elem => 13789110210920521593631520508978233657055078076298943265723336393165330220949
//@ run-call: Cbool_0::t_local_encode_struct_raw_field => 75506153327051474587906755573858019282972751592871715030499431892688993766217
//@ run-call: Cbool_0::t_retdirect_encode_struct_raw_field => 75506153327051474587906755573858019282972751592871715030499431892688993766217
//@ run-call: Cbool_0::t_param_encode_struct_raw_field => 75506153327051474587906755573858019282972751592871715030499431892688993766217
//@ run-call: Cbool_0::t_mem_encode_struct_raw_field => 75506153327051474587906755573858019282972751592871715030499431892688993766217
//@ run-call-fail: Cen_0::t_local_encode => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_encode_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_encode_array => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode_array => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode_array => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode_array => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_encode_struct => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode_struct => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode_struct => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode_struct => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_encode_array_raw_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode_array_raw_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode_array_raw_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode_array_raw_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_encode_struct_raw_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode_struct_raw_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode_struct_raw_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode_struct_raw_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cud_0::t_retdirect_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cud_0::t_param_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cud_0::t_mem_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cud_0::t_local_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cud_0::t_retdirect_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cud_0::t_param_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cud_0::t_mem_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cud_0::t_local_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cud_0::t_retdirect_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cud_0::t_param_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cud_0::t_mem_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cud_0::t_local_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cud_0::t_retdirect_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cud_0::t_param_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cud_0::t_mem_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cud_0::t_local_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cud_0::t_retdirect_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cud_0::t_param_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cud_0::t_mem_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cud_0::t_local_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Cud_0::t_retdirect_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Cud_0::t_param_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Cud_0::t_mem_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874

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

struct St_bool {
    uint256 a;
    bool f;
}

struct St_en {
    uint256 a;
    E f;
}

struct St_ud {
    uint256 a;
    U f;
}

contract Cbool_0 {
    function _s_retdirect_encode() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode(bool pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode_two(bool pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(false, pv, false))); }
    function _s_retdirect_encode_array() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode_array(bool pv) internal view  returns (uint256) { bool[] memory arr3 = new bool[](2); arr3[0] = pv; arr3[1] = false; return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode_struct(bool pv) internal view  returns (uint256) { St_bool memory s = St_bool(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode_array_raw_elem(bool pv) internal view  returns (uint256) { bool[] memory arr3 = new bool[](1); assembly { mstore(add(arr3, 32), 0x100) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode_struct_raw_field(bool pv) internal view  returns (uint256) { St_bool memory s; assembly { mstore(add(s, 32), 0x100) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(keccak256(abi.encode(false, rx, false))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(false, _s_retdirect_encode_two(), false))); }
    function t_param_encode_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(keccak256(abi.encode(false, ry, false))); }
    function t_local_encode_array() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool[] memory arr3 = new bool[](2); arr3[0] = rx; arr3[1] = false; return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { bool[] memory arr3 = new bool[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = false; return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[] memory arr3 = new bool[](2); arr3[0] = ry; arr3[1] = false; return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } St_bool memory s = St_bool(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_bool memory s = St_bool(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; St_bool memory s = St_bool(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool[] memory arr3 = new bool[](1); assembly { mstore(add(arr3, 32), 0x100) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { bool[] memory arr3 = new bool[](1); assembly { mstore(add(arr3, 32), 0x100) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[] memory arr3 = new bool[](1); assembly { mstore(add(arr3, 32), 0x100) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } St_bool memory s; assembly { mstore(add(s, 32), 0x100) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_bool memory s; assembly { mstore(add(s, 32), 0x100) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; St_bool memory s; assembly { mstore(add(s, 32), 0x100) } return uint256(keccak256(abi.encode(s))); }
}
contract Cen_0 {
    function _s_retdirect_encode() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode(E pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode_two(E pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(E.A, pv, E.A))); }
    function _s_retdirect_encode_array() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode_array(E pv) internal view  returns (uint256) { E[] memory arr3 = new E[](2); arr3[0] = pv; arr3[1] = E.A; return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode_struct(E pv) internal view  returns (uint256) { St_en memory s = St_en(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode_array_raw_elem(E pv) internal view  returns (uint256) { E[] memory arr3 = new E[](1); assembly { mstore(add(arr3, 32), 0x101) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode_struct_raw_field(E pv) internal view  returns (uint256) { St_en memory s; assembly { mstore(add(s, 32), 0x101) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(keccak256(abi.encode(E.A, rx, E.A))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(E.A, _s_retdirect_encode_two(), E.A))); }
    function t_param_encode_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(keccak256(abi.encode(E.A, ry, E.A))); }
    function t_local_encode_array() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E[] memory arr3 = new E[](2); arr3[0] = rx; arr3[1] = E.A; return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { E[] memory arr3 = new E[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = E.A; return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[] memory arr3 = new E[](2); arr3[0] = ry; arr3[1] = E.A; return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { E rx; assembly { rx := 0x101 } St_en memory s = St_en(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_en memory s = St_en(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; St_en memory s = St_en(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E[] memory arr3 = new E[](1); assembly { mstore(add(arr3, 32), 0x101) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { E[] memory arr3 = new E[](1); assembly { mstore(add(arr3, 32), 0x101) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[] memory arr3 = new E[](1); assembly { mstore(add(arr3, 32), 0x101) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { E rx; assembly { rx := 0x101 } St_en memory s; assembly { mstore(add(s, 32), 0x101) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_en memory s; assembly { mstore(add(s, 32), 0x101) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; St_en memory s; assembly { mstore(add(s, 32), 0x101) } return uint256(keccak256(abi.encode(s))); }
}
contract Cud_0 {
    function _s_retdirect_encode() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode(U pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_two(U pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(U.wrap(3), pv, U.wrap(3)))); }
    function _s_retdirect_encode_array() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_array(U pv) internal view  returns (uint256) { U[] memory arr3 = new U[](2); arr3[0] = pv; arr3[1] = U.wrap(3); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_struct(U pv) internal view  returns (uint256) { St_ud memory s = St_ud(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_array_raw_elem(U pv) internal view  returns (uint256) { U[] memory arr3 = new U[](1); assembly { mstore(add(arr3, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_struct_raw_field(U pv) internal view  returns (uint256) { St_ud memory s; assembly { mstore(add(s, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encode(U.wrap(3), rx, U.wrap(3)))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(U.wrap(3), _s_retdirect_encode_two(), U.wrap(3)))); }
    function t_param_encode_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(keccak256(abi.encode(U.wrap(3), ry, U.wrap(3)))); }
    function t_local_encode_array() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[] memory arr3 = new U[](2); arr3[0] = rx; arr3[1] = U.wrap(3); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { U[] memory arr3 = new U[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = U.wrap(3); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[] memory arr3 = new U[](2); arr3[0] = ry; arr3[1] = U.wrap(3); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } St_ud memory s = St_ud(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_ud memory s = St_ud(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; St_ud memory s = St_ud(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[] memory arr3 = new U[](1); assembly { mstore(add(arr3, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { U[] memory arr3 = new U[](1); assembly { mstore(add(arr3, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[] memory arr3 = new U[](1); assembly { mstore(add(arr3, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } St_ud memory s; assembly { mstore(add(s, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_ud memory s; assembly { mstore(add(s, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; St_ud memory s; assembly { mstore(add(s, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(s))); }
}
