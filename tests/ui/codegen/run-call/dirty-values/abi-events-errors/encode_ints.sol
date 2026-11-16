//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cu8_0::t_retdirect_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cu8_0::t_param_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cu8_0::t_mem_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cu8_0::t_local_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cu8_0::t_retdirect_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cu8_0::t_param_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cu8_0::t_mem_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cu8_0::t_local_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cu8_0::t_retdirect_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cu8_0::t_param_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cu8_0::t_mem_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cu8_0::t_local_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cu8_0::t_retdirect_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cu8_0::t_param_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cu8_0::t_mem_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cu8_0::t_local_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cu8_0::t_retdirect_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cu8_0::t_param_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cu8_0::t_mem_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cu8_0::t_local_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Cu8_0::t_retdirect_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Cu8_0::t_param_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Cu8_0::t_mem_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Cu64_0::t_local_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cu64_0::t_retdirect_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cu64_0::t_param_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cu64_0::t_mem_encode => 26616959624788145501257665028759500958019551560565664426117466910078421933505
//@ run-call: Cu64_0::t_local_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cu64_0::t_retdirect_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cu64_0::t_param_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cu64_0::t_mem_encode_two => 5586836831208764130169984769263105402598882306144294801643202912876312056058
//@ run-call: Cu64_0::t_local_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cu64_0::t_retdirect_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cu64_0::t_param_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cu64_0::t_mem_encode_array => 37739699472345149633170537670743902067011739942734700633063005861712533112902
//@ run-call: Cu64_0::t_local_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cu64_0::t_retdirect_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cu64_0::t_param_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cu64_0::t_mem_encode_struct => 72228337379227784915664204743802217409078105410611160856693656298806718554115
//@ run-call: Cu64_0::t_local_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cu64_0::t_retdirect_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cu64_0::t_param_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cu64_0::t_mem_encode_array_raw_elem => 20507483988048299401288417388362975531044209454247833253819392490630181377664
//@ run-call: Cu64_0::t_local_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Cu64_0::t_retdirect_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Cu64_0::t_param_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Cu64_0::t_mem_encode_struct_raw_field => 39943600054911156392256153131035100008904409526959966566194127862912790832874
//@ run-call: Ci8_0::t_local_encode => 7837748519300134473946677271829657694082751735497144148692609999357060131213
//@ run-call: Ci8_0::t_retdirect_encode => 7837748519300134473946677271829657694082751735497144148692609999357060131213
//@ run-call: Ci8_0::t_param_encode => 7837748519300134473946677271829657694082751735497144148692609999357060131213
//@ run-call: Ci8_0::t_mem_encode => 7837748519300134473946677271829657694082751735497144148692609999357060131213
//@ run-call: Ci8_0::t_local_encode_two => 5153560378014306949204393375688572190268788064344995310554253552551034824560
//@ run-call: Ci8_0::t_retdirect_encode_two => 5153560378014306949204393375688572190268788064344995310554253552551034824560
//@ run-call: Ci8_0::t_param_encode_two => 5153560378014306949204393375688572190268788064344995310554253552551034824560
//@ run-call: Ci8_0::t_mem_encode_two => 5153560378014306949204393375688572190268788064344995310554253552551034824560
//@ run-call: Ci8_0::t_local_encode_array => 13453165936766214490036206812271509468074215955432135619585616630092453961904
//@ run-call: Ci8_0::t_retdirect_encode_array => 13453165936766214490036206812271509468074215955432135619585616630092453961904
//@ run-call: Ci8_0::t_param_encode_array => 13453165936766214490036206812271509468074215955432135619585616630092453961904
//@ run-call: Ci8_0::t_mem_encode_array => 13453165936766214490036206812271509468074215955432135619585616630092453961904
//@ run-call: Ci8_0::t_local_encode_struct => 25286521332888869944195462067803283909761543239004523391836916369972343512255
//@ run-call: Ci8_0::t_retdirect_encode_struct => 25286521332888869944195462067803283909761543239004523391836916369972343512255
//@ run-call: Ci8_0::t_param_encode_struct => 25286521332888869944195462067803283909761543239004523391836916369972343512255
//@ run-call: Ci8_0::t_mem_encode_struct => 25286521332888869944195462067803283909761543239004523391836916369972343512255
//@ run-call: Ci8_0::t_local_encode_array_raw_elem => 76037763290927412676775079620557201937606854920625751867623647988026131296821
//@ run-call: Ci8_0::t_retdirect_encode_array_raw_elem => 76037763290927412676775079620557201937606854920625751867623647988026131296821
//@ run-call: Ci8_0::t_param_encode_array_raw_elem => 76037763290927412676775079620557201937606854920625751867623647988026131296821
//@ run-call: Ci8_0::t_mem_encode_array_raw_elem => 76037763290927412676775079620557201937606854920625751867623647988026131296821
//@ run-call: Ci8_0::t_local_encode_struct_raw_field => 64538792394448693612781593303873494388772949353675327337004217679117205282687
//@ run-call: Ci8_0::t_retdirect_encode_struct_raw_field => 64538792394448693612781593303873494388772949353675327337004217679117205282687
//@ run-call: Ci8_0::t_param_encode_struct_raw_field => 64538792394448693612781593303873494388772949353675327337004217679117205282687
//@ run-call: Ci8_0::t_mem_encode_struct_raw_field => 64538792394448693612781593303873494388772949353675327337004217679117205282687
//@ run-call: Ci24_0::t_local_encode => 7837748519300134473946677271829657694082751735497144148692609999357060131213
//@ run-call: Ci24_0::t_retdirect_encode => 7837748519300134473946677271829657694082751735497144148692609999357060131213
//@ run-call: Ci24_0::t_param_encode => 7837748519300134473946677271829657694082751735497144148692609999357060131213
//@ run-call: Ci24_0::t_mem_encode => 7837748519300134473946677271829657694082751735497144148692609999357060131213
//@ run-call: Ci24_0::t_local_encode_two => 5153560378014306949204393375688572190268788064344995310554253552551034824560
//@ run-call: Ci24_0::t_retdirect_encode_two => 5153560378014306949204393375688572190268788064344995310554253552551034824560
//@ run-call: Ci24_0::t_param_encode_two => 5153560378014306949204393375688572190268788064344995310554253552551034824560
//@ run-call: Ci24_0::t_mem_encode_two => 5153560378014306949204393375688572190268788064344995310554253552551034824560
//@ run-call: Ci24_0::t_local_encode_array => 13453165936766214490036206812271509468074215955432135619585616630092453961904
//@ run-call: Ci24_0::t_retdirect_encode_array => 13453165936766214490036206812271509468074215955432135619585616630092453961904
//@ run-call: Ci24_0::t_param_encode_array => 13453165936766214490036206812271509468074215955432135619585616630092453961904
//@ run-call: Ci24_0::t_mem_encode_array => 13453165936766214490036206812271509468074215955432135619585616630092453961904
//@ run-call: Ci24_0::t_local_encode_struct => 25286521332888869944195462067803283909761543239004523391836916369972343512255
//@ run-call: Ci24_0::t_retdirect_encode_struct => 25286521332888869944195462067803283909761543239004523391836916369972343512255
//@ run-call: Ci24_0::t_param_encode_struct => 25286521332888869944195462067803283909761543239004523391836916369972343512255
//@ run-call: Ci24_0::t_mem_encode_struct => 25286521332888869944195462067803283909761543239004523391836916369972343512255
//@ run-call: Ci24_0::t_local_encode_array_raw_elem => 76037763290927412676775079620557201937606854920625751867623647988026131296821
//@ run-call: Ci24_0::t_retdirect_encode_array_raw_elem => 76037763290927412676775079620557201937606854920625751867623647988026131296821
//@ run-call: Ci24_0::t_param_encode_array_raw_elem => 76037763290927412676775079620557201937606854920625751867623647988026131296821
//@ run-call: Ci24_0::t_mem_encode_array_raw_elem => 76037763290927412676775079620557201937606854920625751867623647988026131296821
//@ run-call: Ci24_0::t_local_encode_struct_raw_field => 64538792394448693612781593303873494388772949353675327337004217679117205282687
//@ run-call: Ci24_0::t_retdirect_encode_struct_raw_field => 64538792394448693612781593303873494388772949353675327337004217679117205282687
//@ run-call: Ci24_0::t_param_encode_struct_raw_field => 64538792394448693612781593303873494388772949353675327337004217679117205282687
//@ run-call: Ci24_0::t_mem_encode_struct_raw_field => 64538792394448693612781593303873494388772949353675327337004217679117205282687

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

struct St_u8 {
    uint256 a;
    uint8 f;
}

struct St_u64 {
    uint256 a;
    uint64 f;
}

struct St_i8 {
    uint256 a;
    int8 f;
}

struct St_i24 {
    uint256 a;
    int24 f;
}

contract Cu8_0 {
    function _s_retdirect_encode() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_two(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(uint8(3), pv, uint8(3)))); }
    function _s_retdirect_encode_array() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_array(uint8 pv) internal view  returns (uint256) { uint8[] memory arr3 = new uint8[](2); arr3[0] = pv; arr3[1] = uint8(3); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_struct(uint8 pv) internal view  returns (uint256) { St_u8 memory s = St_u8(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_array_raw_elem(uint8 pv) internal view  returns (uint256) { uint8[] memory arr3 = new uint8[](1); assembly { mstore(add(arr3, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_struct_raw_field(uint8 pv) internal view  returns (uint256) { St_u8 memory s; assembly { mstore(add(s, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encode(uint8(3), rx, uint8(3)))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(uint8(3), _s_retdirect_encode_two(), uint8(3)))); }
    function t_param_encode_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encode(uint8(3), ry, uint8(3)))); }
    function t_local_encode_array() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[] memory arr3 = new uint8[](2); arr3[0] = rx; arr3[1] = uint8(3); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { uint8[] memory arr3 = new uint8[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = uint8(3); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[] memory arr3 = new uint8[](2); arr3[0] = ry; arr3[1] = uint8(3); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } St_u8 memory s = St_u8(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_u8 memory s = St_u8(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; St_u8 memory s = St_u8(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[] memory arr3 = new uint8[](1); assembly { mstore(add(arr3, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { uint8[] memory arr3 = new uint8[](1); assembly { mstore(add(arr3, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[] memory arr3 = new uint8[](1); assembly { mstore(add(arr3, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } St_u8 memory s; assembly { mstore(add(s, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_u8 memory s; assembly { mstore(add(s, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; St_u8 memory s; assembly { mstore(add(s, 32), 0xabcd1b) } return uint256(keccak256(abi.encode(s))); }
}
contract Cu64_0 {
    function _s_retdirect_encode() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode_two(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(uint64(3), pv, uint64(3)))); }
    function _s_retdirect_encode_array() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode_array(uint64 pv) internal view  returns (uint256) { uint64[] memory arr3 = new uint64[](2); arr3[0] = pv; arr3[1] = uint64(3); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode_struct(uint64 pv) internal view  returns (uint256) { St_u64 memory s = St_u64(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode_array_raw_elem(uint64 pv) internal view  returns (uint256) { uint64[] memory arr3 = new uint64[](1); assembly { mstore(add(arr3, 32), 0xabcd000000000000001b) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode_struct_raw_field(uint64 pv) internal view  returns (uint256) { St_u64 memory s; assembly { mstore(add(s, 32), 0xabcd000000000000001b) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encode(uint64(3), rx, uint64(3)))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(uint64(3), _s_retdirect_encode_two(), uint64(3)))); }
    function t_param_encode_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encode(uint64(3), ry, uint64(3)))); }
    function t_local_encode_array() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[] memory arr3 = new uint64[](2); arr3[0] = rx; arr3[1] = uint64(3); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { uint64[] memory arr3 = new uint64[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = uint64(3); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[] memory arr3 = new uint64[](2); arr3[0] = ry; arr3[1] = uint64(3); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } St_u64 memory s = St_u64(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_u64 memory s = St_u64(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; St_u64 memory s = St_u64(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[] memory arr3 = new uint64[](1); assembly { mstore(add(arr3, 32), 0xabcd000000000000001b) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { uint64[] memory arr3 = new uint64[](1); assembly { mstore(add(arr3, 32), 0xabcd000000000000001b) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[] memory arr3 = new uint64[](1); assembly { mstore(add(arr3, 32), 0xabcd000000000000001b) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } St_u64 memory s; assembly { mstore(add(s, 32), 0xabcd000000000000001b) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_u64 memory s; assembly { mstore(add(s, 32), 0xabcd000000000000001b) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; St_u64 memory s; assembly { mstore(add(s, 32), 0xabcd000000000000001b) } return uint256(keccak256(abi.encode(s))); }
}
contract Ci8_0 {
    function _s_retdirect_encode() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode_two(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(int8(-2), pv, int8(-2)))); }
    function _s_retdirect_encode_array() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode_array(int8 pv) internal view  returns (uint256) { int8[] memory arr3 = new int8[](2); arr3[0] = pv; arr3[1] = int8(-2); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode_struct(int8 pv) internal view  returns (uint256) { St_i8 memory s = St_i8(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode_array_raw_elem(int8 pv) internal view  returns (uint256) { int8[] memory arr3 = new int8[](1); assembly { mstore(add(arr3, 32), 0xabcdfd) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode_struct_raw_field(int8 pv) internal view  returns (uint256) { St_i8 memory s; assembly { mstore(add(s, 32), 0xabcdfd) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encode(int8(-2), rx, int8(-2)))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(int8(-2), _s_retdirect_encode_two(), int8(-2)))); }
    function t_param_encode_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encode(int8(-2), ry, int8(-2)))); }
    function t_local_encode_array() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[] memory arr3 = new int8[](2); arr3[0] = rx; arr3[1] = int8(-2); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { int8[] memory arr3 = new int8[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = int8(-2); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[] memory arr3 = new int8[](2); arr3[0] = ry; arr3[1] = int8(-2); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } St_i8 memory s = St_i8(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_i8 memory s = St_i8(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; St_i8 memory s = St_i8(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[] memory arr3 = new int8[](1); assembly { mstore(add(arr3, 32), 0xabcdfd) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { int8[] memory arr3 = new int8[](1); assembly { mstore(add(arr3, 32), 0xabcdfd) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[] memory arr3 = new int8[](1); assembly { mstore(add(arr3, 32), 0xabcdfd) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } St_i8 memory s; assembly { mstore(add(s, 32), 0xabcdfd) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_i8 memory s; assembly { mstore(add(s, 32), 0xabcdfd) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; St_i8 memory s; assembly { mstore(add(s, 32), 0xabcdfd) } return uint256(keccak256(abi.encode(s))); }
}
contract Ci24_0 {
    function _s_retdirect_encode() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode_two(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(int24(-2), pv, int24(-2)))); }
    function _s_retdirect_encode_array() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode_array(int24 pv) internal view  returns (uint256) { int24[] memory arr3 = new int24[](2); arr3[0] = pv; arr3[1] = int24(-2); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode_struct(int24 pv) internal view  returns (uint256) { St_i24 memory s = St_i24(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode_array_raw_elem(int24 pv) internal view  returns (uint256) { int24[] memory arr3 = new int24[](1); assembly { mstore(add(arr3, 32), 0xabcdfffffd) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode_struct_raw_field(int24 pv) internal view  returns (uint256) { St_i24 memory s; assembly { mstore(add(s, 32), 0xabcdfffffd) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encode(int24(-2), rx, int24(-2)))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(int24(-2), _s_retdirect_encode_two(), int24(-2)))); }
    function t_param_encode_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encode(int24(-2), ry, int24(-2)))); }
    function t_local_encode_array() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[] memory arr3 = new int24[](2); arr3[0] = rx; arr3[1] = int24(-2); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { int24[] memory arr3 = new int24[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = int24(-2); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[] memory arr3 = new int24[](2); arr3[0] = ry; arr3[1] = int24(-2); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } St_i24 memory s = St_i24(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_i24 memory s = St_i24(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; St_i24 memory s = St_i24(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[] memory arr3 = new int24[](1); assembly { mstore(add(arr3, 32), 0xabcdfffffd) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { int24[] memory arr3 = new int24[](1); assembly { mstore(add(arr3, 32), 0xabcdfffffd) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[] memory arr3 = new int24[](1); assembly { mstore(add(arr3, 32), 0xabcdfffffd) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } St_i24 memory s; assembly { mstore(add(s, 32), 0xabcdfffffd) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_i24 memory s; assembly { mstore(add(s, 32), 0xabcdfffffd) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; St_i24 memory s; assembly { mstore(add(s, 32), 0xabcdfffffd) } return uint256(keccak256(abi.encode(s))); }
}
