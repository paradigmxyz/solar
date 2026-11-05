//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cb1_0::t_local_encode => 74576347757645214927310446075331516632198131166908332713107829342027104355102
//@ run-call: Cb1_0::t_retdirect_encode => 74576347757645214927310446075331516632198131166908332713107829342027104355102
//@ run-call: Cb1_0::t_param_encode => 74576347757645214927310446075331516632198131166908332713107829342027104355102
//@ run-call: Cb1_0::t_mem_encode => 74576347757645214927310446075331516632198131166908332713107829342027104355102
//@ run-call: Cb1_0::t_local_encode_two => 54652159734953900029515808704127792245102745878804966320899331643327928866100
//@ run-call: Cb1_0::t_retdirect_encode_two => 54652159734953900029515808704127792245102745878804966320899331643327928866100
//@ run-call: Cb1_0::t_param_encode_two => 54652159734953900029515808704127792245102745878804966320899331643327928866100
//@ run-call: Cb1_0::t_mem_encode_two => 54652159734953900029515808704127792245102745878804966320899331643327928866100
//@ run-call: Cb1_0::t_local_encode_array => 24841586988327006227942984345627868625933515216751856430625592140169586199119
//@ run-call: Cb1_0::t_retdirect_encode_array => 24841586988327006227942984345627868625933515216751856430625592140169586199119
//@ run-call: Cb1_0::t_param_encode_array => 24841586988327006227942984345627868625933515216751856430625592140169586199119
//@ run-call: Cb1_0::t_mem_encode_array => 24841586988327006227942984345627868625933515216751856430625592140169586199119
//@ run-call: Cb1_0::t_local_encode_struct => 90056625398946035395079611146144299262355275788891419672653713109291210765674
//@ run-call: Cb1_0::t_retdirect_encode_struct => 90056625398946035395079611146144299262355275788891419672653713109291210765674
//@ run-call: Cb1_0::t_param_encode_struct => 90056625398946035395079611146144299262355275788891419672653713109291210765674
//@ run-call: Cb1_0::t_mem_encode_struct => 90056625398946035395079611146144299262355275788891419672653713109291210765674
//@ run-call: Cb1_0::t_local_encode_array_raw_elem => 114759424049882741570604472257508076197633556382518785125151223343960020020680
//@ run-call: Cb1_0::t_retdirect_encode_array_raw_elem => 114759424049882741570604472257508076197633556382518785125151223343960020020680
//@ run-call: Cb1_0::t_param_encode_array_raw_elem => 114759424049882741570604472257508076197633556382518785125151223343960020020680
//@ run-call: Cb1_0::t_mem_encode_array_raw_elem => 114759424049882741570604472257508076197633556382518785125151223343960020020680
//@ run-call: Cb1_0::t_local_encode_struct_raw_field => 74081334944035311143821658470326809077232872563047654910478735599333551596826
//@ run-call: Cb1_0::t_retdirect_encode_struct_raw_field => 74081334944035311143821658470326809077232872563047654910478735599333551596826
//@ run-call: Cb1_0::t_param_encode_struct_raw_field => 74081334944035311143821658470326809077232872563047654910478735599333551596826
//@ run-call: Cb1_0::t_mem_encode_struct_raw_field => 74081334944035311143821658470326809077232872563047654910478735599333551596826
//@ run-call: Cb4_0::t_local_encode => 7607765980783314147439994338015680210193044234666091072709192533829234598717
//@ run-call: Cb4_0::t_retdirect_encode => 7607765980783314147439994338015680210193044234666091072709192533829234598717
//@ run-call: Cb4_0::t_param_encode => 7607765980783314147439994338015680210193044234666091072709192533829234598717
//@ run-call: Cb4_0::t_mem_encode => 7607765980783314147439994338015680210193044234666091072709192533829234598717
//@ run-call: Cb4_0::t_local_encode_two => 92116530602146020690774375414981772967041335426141717026643352986114315347447
//@ run-call: Cb4_0::t_retdirect_encode_two => 92116530602146020690774375414981772967041335426141717026643352986114315347447
//@ run-call: Cb4_0::t_param_encode_two => 92116530602146020690774375414981772967041335426141717026643352986114315347447
//@ run-call: Cb4_0::t_mem_encode_two => 92116530602146020690774375414981772967041335426141717026643352986114315347447
//@ run-call: Cb4_0::t_local_encode_array => 65026752063978414074343510022263461019971929199856715308843619044267695136215
//@ run-call: Cb4_0::t_retdirect_encode_array => 65026752063978414074343510022263461019971929199856715308843619044267695136215
//@ run-call: Cb4_0::t_param_encode_array => 65026752063978414074343510022263461019971929199856715308843619044267695136215
//@ run-call: Cb4_0::t_mem_encode_array => 65026752063978414074343510022263461019971929199856715308843619044267695136215
//@ run-call: Cb4_0::t_local_encode_struct => 27353572685409868835213973237652455152016071612386969033242954986111972039085
//@ run-call: Cb4_0::t_retdirect_encode_struct => 27353572685409868835213973237652455152016071612386969033242954986111972039085
//@ run-call: Cb4_0::t_param_encode_struct => 27353572685409868835213973237652455152016071612386969033242954986111972039085
//@ run-call: Cb4_0::t_mem_encode_struct => 27353572685409868835213973237652455152016071612386969033242954986111972039085
//@ run-call: Cb4_0::t_local_encode_array_raw_elem => 73313708302237590585308657835624659839610549070157751776393869011404854290520
//@ run-call: Cb4_0::t_retdirect_encode_array_raw_elem => 73313708302237590585308657835624659839610549070157751776393869011404854290520
//@ run-call: Cb4_0::t_param_encode_array_raw_elem => 73313708302237590585308657835624659839610549070157751776393869011404854290520
//@ run-call: Cb4_0::t_mem_encode_array_raw_elem => 73313708302237590585308657835624659839610549070157751776393869011404854290520
//@ run-call: Cb4_0::t_local_encode_struct_raw_field => 85522512628069312694981319191960505083536848781230536882272179811159734697110
//@ run-call: Cb4_0::t_retdirect_encode_struct_raw_field => 85522512628069312694981319191960505083536848781230536882272179811159734697110
//@ run-call: Cb4_0::t_param_encode_struct_raw_field => 85522512628069312694981319191960505083536848781230536882272179811159734697110
//@ run-call: Cb4_0::t_mem_encode_struct_raw_field => 85522512628069312694981319191960505083536848781230536882272179811159734697110
//@ run-call: Cb20_0::t_local_encode => 68780661181532719737141533639632850092553555322381873261168292375575348261245
//@ run-call: Cb20_0::t_retdirect_encode => 68780661181532719737141533639632850092553555322381873261168292375575348261245
//@ run-call: Cb20_0::t_param_encode => 68780661181532719737141533639632850092553555322381873261168292375575348261245
//@ run-call: Cb20_0::t_mem_encode => 68780661181532719737141533639632850092553555322381873261168292375575348261245
//@ run-call: Cb20_0::t_local_encode_two => 74694196340905254439580066707147377562971737192683592604236057648433461601213
//@ run-call: Cb20_0::t_retdirect_encode_two => 74694196340905254439580066707147377562971737192683592604236057648433461601213
//@ run-call: Cb20_0::t_param_encode_two => 74694196340905254439580066707147377562971737192683592604236057648433461601213
//@ run-call: Cb20_0::t_mem_encode_two => 74694196340905254439580066707147377562971737192683592604236057648433461601213
//@ run-call: Cb20_0::t_local_encode_array => 73146576413130871931969243613919925295036205465668229283194292004688603020511
//@ run-call: Cb20_0::t_retdirect_encode_array => 73146576413130871931969243613919925295036205465668229283194292004688603020511
//@ run-call: Cb20_0::t_param_encode_array => 73146576413130871931969243613919925295036205465668229283194292004688603020511
//@ run-call: Cb20_0::t_mem_encode_array => 73146576413130871931969243613919925295036205465668229283194292004688603020511
//@ run-call: Cb20_0::t_local_encode_struct => 63008316577794128990516848337124545765428914287961043056698033199209512742830
//@ run-call: Cb20_0::t_retdirect_encode_struct => 63008316577794128990516848337124545765428914287961043056698033199209512742830
//@ run-call: Cb20_0::t_param_encode_struct => 63008316577794128990516848337124545765428914287961043056698033199209512742830
//@ run-call: Cb20_0::t_mem_encode_struct => 63008316577794128990516848337124545765428914287961043056698033199209512742830
//@ run-call: Cb20_0::t_local_encode_array_raw_elem => 15371691557033489851087213310115828176421162476107376193118029144636651400140
//@ run-call: Cb20_0::t_retdirect_encode_array_raw_elem => 15371691557033489851087213310115828176421162476107376193118029144636651400140
//@ run-call: Cb20_0::t_param_encode_array_raw_elem => 15371691557033489851087213310115828176421162476107376193118029144636651400140
//@ run-call: Cb20_0::t_mem_encode_array_raw_elem => 15371691557033489851087213310115828176421162476107376193118029144636651400140
//@ run-call: Cb20_0::t_local_encode_struct_raw_field => 92135988310412341394385837144410044078570317778430121874501950207757815348376
//@ run-call: Cb20_0::t_retdirect_encode_struct_raw_field => 92135988310412341394385837144410044078570317778430121874501950207757815348376
//@ run-call: Cb20_0::t_param_encode_struct_raw_field => 92135988310412341394385837144410044078570317778430121874501950207757815348376
//@ run-call: Cb20_0::t_mem_encode_struct_raw_field => 92135988310412341394385837144410044078570317778430121874501950207757815348376

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

struct St_b1 {
    uint256 a;
    bytes1 f;
}

struct St_b4 {
    uint256 a;
    bytes4 f;
}

struct St_b20 {
    uint256 a;
    bytes20 f;
}

contract Cb1_0 {
    function _s_retdirect_encode() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode(bytes1 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode_two(bytes1 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(bytes1(0xcd), pv, bytes1(0xcd)))); }
    function _s_retdirect_encode_array() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode_array(bytes1 pv) internal view  returns (uint256) { bytes1[] memory arr3 = new bytes1[](2); arr3[0] = pv; arr3[1] = bytes1(0xcd); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode_struct(bytes1 pv) internal view  returns (uint256) { St_b1 memory s = St_b1(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode_array_raw_elem(bytes1 pv) internal view  returns (uint256) { bytes1[] memory arr3 = new bytes1[](1); assembly { mstore(add(arr3, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode_struct_raw_field(bytes1 pv) internal view  returns (uint256) { St_b1 memory s; assembly { mstore(add(s, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(keccak256(abi.encode(bytes1(0xcd), rx, bytes1(0xcd)))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(bytes1(0xcd), _s_retdirect_encode_two(), bytes1(0xcd)))); }
    function t_param_encode_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(keccak256(abi.encode(bytes1(0xcd), ry, bytes1(0xcd)))); }
    function t_local_encode_array() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[] memory arr3 = new bytes1[](2); arr3[0] = rx; arr3[1] = bytes1(0xcd); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { bytes1[] memory arr3 = new bytes1[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = bytes1(0xcd); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[] memory arr3 = new bytes1[](2); arr3[0] = ry; arr3[1] = bytes1(0xcd); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } St_b1 memory s = St_b1(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_b1 memory s = St_b1(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; St_b1 memory s = St_b1(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[] memory arr3 = new bytes1[](1); assembly { mstore(add(arr3, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { bytes1[] memory arr3 = new bytes1[](1); assembly { mstore(add(arr3, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[] memory arr3 = new bytes1[](1); assembly { mstore(add(arr3, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } St_b1 memory s; assembly { mstore(add(s, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_b1 memory s; assembly { mstore(add(s, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; St_b1 memory s; assembly { mstore(add(s, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } return uint256(keccak256(abi.encode(s))); }
}
contract Cb4_0 {
    function _s_retdirect_encode() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode(bytes4 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode_two(bytes4 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(bytes4(0x11223344), pv, bytes4(0x11223344)))); }
    function _s_retdirect_encode_array() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode_array(bytes4 pv) internal view  returns (uint256) { bytes4[] memory arr3 = new bytes4[](2); arr3[0] = pv; arr3[1] = bytes4(0x11223344); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode_struct(bytes4 pv) internal view  returns (uint256) { St_b4 memory s = St_b4(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode_array_raw_elem(bytes4 pv) internal view  returns (uint256) { bytes4[] memory arr3 = new bytes4[](1); assembly { mstore(add(arr3, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode_struct_raw_field(bytes4 pv) internal view  returns (uint256) { St_b4 memory s; assembly { mstore(add(s, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(keccak256(abi.encode(bytes4(0x11223344), rx, bytes4(0x11223344)))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(bytes4(0x11223344), _s_retdirect_encode_two(), bytes4(0x11223344)))); }
    function t_param_encode_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(keccak256(abi.encode(bytes4(0x11223344), ry, bytes4(0x11223344)))); }
    function t_local_encode_array() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[] memory arr3 = new bytes4[](2); arr3[0] = rx; arr3[1] = bytes4(0x11223344); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { bytes4[] memory arr3 = new bytes4[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = bytes4(0x11223344); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[] memory arr3 = new bytes4[](2); arr3[0] = ry; arr3[1] = bytes4(0x11223344); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } St_b4 memory s = St_b4(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_b4 memory s = St_b4(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; St_b4 memory s = St_b4(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[] memory arr3 = new bytes4[](1); assembly { mstore(add(arr3, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { bytes4[] memory arr3 = new bytes4[](1); assembly { mstore(add(arr3, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[] memory arr3 = new bytes4[](1); assembly { mstore(add(arr3, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } St_b4 memory s; assembly { mstore(add(s, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_b4 memory s; assembly { mstore(add(s, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; St_b4 memory s; assembly { mstore(add(s, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } return uint256(keccak256(abi.encode(s))); }
}
contract Cb20_0 {
    function _s_retdirect_encode() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode(bytes20 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv))); }
    function _s_retdirect_encode_two() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode_two(bytes20 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(bytes20(0x1111111111111111111111111111111111111111), pv, bytes20(0x1111111111111111111111111111111111111111)))); }
    function _s_retdirect_encode_array() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode_array(bytes20 pv) internal view  returns (uint256) { bytes20[] memory arr3 = new bytes20[](2); arr3[0] = pv; arr3[1] = bytes20(0x1111111111111111111111111111111111111111); return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode_struct(bytes20 pv) internal view  returns (uint256) { St_b20 memory s = St_b20(1, pv); return uint256(keccak256(abi.encode(s))); }
    function _s_retdirect_encode_array_raw_elem() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode_array_raw_elem(bytes20 pv) internal view  returns (uint256) { bytes20[] memory arr3 = new bytes20[](1); assembly { mstore(add(arr3, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } return uint256(keccak256(abi.encode(arr3))); }
    function _s_retdirect_encode_struct_raw_field() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode_struct_raw_field(bytes20 pv) internal view  returns (uint256) { St_b20 memory s; assembly { mstore(add(s, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } return uint256(keccak256(abi.encode(s))); }
    function t_local_encode() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(keccak256(abi.encode(rx))); }
    function t_retdirect_encode() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_encode()))); }
    function t_param_encode() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode(rx); }
    function t_mem_encode() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(keccak256(abi.encode(ry))); }
    function t_local_encode_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(keccak256(abi.encode(bytes20(0x1111111111111111111111111111111111111111), rx, bytes20(0x1111111111111111111111111111111111111111)))); }
    function t_retdirect_encode_two() external view  returns (uint256) { return uint256(keccak256(abi.encode(bytes20(0x1111111111111111111111111111111111111111), _s_retdirect_encode_two(), bytes20(0x1111111111111111111111111111111111111111)))); }
    function t_param_encode_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode_two(rx); }
    function t_mem_encode_two() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(keccak256(abi.encode(bytes20(0x1111111111111111111111111111111111111111), ry, bytes20(0x1111111111111111111111111111111111111111)))); }
    function t_local_encode_array() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[] memory arr3 = new bytes20[](2); arr3[0] = rx; arr3[1] = bytes20(0x1111111111111111111111111111111111111111); return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array() external view  returns (uint256) { bytes20[] memory arr3 = new bytes20[](2); arr3[0] = _s_retdirect_encode_array(); arr3[1] = bytes20(0x1111111111111111111111111111111111111111); return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode_array(rx); }
    function t_mem_encode_array() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[] memory arr3 = new bytes20[](2); arr3[0] = ry; arr3[1] = bytes20(0x1111111111111111111111111111111111111111); return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } St_b20 memory s = St_b20(1, rx); return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct() external view  returns (uint256) { St_b20 memory s = St_b20(1, _s_retdirect_encode_struct()); return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode_struct(rx); }
    function t_mem_encode_struct() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; St_b20 memory s = St_b20(1, ry); return uint256(keccak256(abi.encode(s))); }
    function t_local_encode_array_raw_elem() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[] memory arr3 = new bytes20[](1); assembly { mstore(add(arr3, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } return uint256(keccak256(abi.encode(arr3))); }
    function t_retdirect_encode_array_raw_elem() external view  returns (uint256) { bytes20[] memory arr3 = new bytes20[](1); assembly { mstore(add(arr3, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } return uint256(keccak256(abi.encode(arr3))); }
    function t_param_encode_array_raw_elem() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode_array_raw_elem(rx); }
    function t_mem_encode_array_raw_elem() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[] memory arr3 = new bytes20[](1); assembly { mstore(add(arr3, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } return uint256(keccak256(abi.encode(arr3))); }
    function t_local_encode_struct_raw_field() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } St_b20 memory s; assembly { mstore(add(s, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } return uint256(keccak256(abi.encode(s))); }
    function t_retdirect_encode_struct_raw_field() external view  returns (uint256) { St_b20 memory s; assembly { mstore(add(s, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } return uint256(keccak256(abi.encode(s))); }
    function t_param_encode_struct_raw_field() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode_struct_raw_field(rx); }
    function t_mem_encode_struct_raw_field() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; St_b20 memory s; assembly { mstore(add(s, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } return uint256(keccak256(abi.encode(s))); }
}
