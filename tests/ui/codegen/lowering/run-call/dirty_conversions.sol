//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_to_uint16 => 27
//@ run-call: Cu8_0::t_retdirect_to_uint16 => 27
//@ run-call: Cu8_0::t_ret_to_uint16 => 27
//@ run-call: Cu8_0::t_param_to_uint16 => 27
//@ run-call: Cu8_0::t_copy_to_uint16 => 27
//@ run-call: Cu8_0::t_ternary_to_uint16 => 27
//@ run-call: Cu8_0::t_ifjoin_to_uint16 => 27
//@ run-call: Cu8_0::t_loopjoin_to_uint16 => 27
//@ run-call: Cu8_0::t_tuple_to_uint16 => 27
//@ run-call: Cu8_0::t_modifier_to_uint16 => 27
//@ run-call: Cu8_0::t_ptr_to_uint16 => 27
//@ run-call: Cu8_0::t_mem_to_uint16 => 27
//@ run-call: Cu8_0::t_local_to_uint64 => 27
//@ run-call: Cu8_0::t_retdirect_to_uint64 => 27
//@ run-call: Cu8_0::t_ret_to_uint64 => 27
//@ run-call: Cu8_0::t_param_to_uint64 => 27
//@ run-call: Cu8_0::t_copy_to_uint64 => 27
//@ run-call: Cu8_0::t_ternary_to_uint64 => 27
//@ run-call: Cu8_0::t_ifjoin_to_uint64 => 27
//@ run-call: Cu8_0::t_loopjoin_to_uint64 => 27
//@ run-call: Cu8_0::t_tuple_to_uint64 => 27
//@ run-call: Cu8_0::t_modifier_to_uint64 => 27
//@ run-call: Cu8_0::t_ptr_to_uint64 => 27
//@ run-call: Cu8_0::t_mem_to_uint64 => 27
//@ run-call: Cu8_0::t_local_to_uint128 => 27
//@ run-call: Cu8_0::t_retdirect_to_uint128 => 27
//@ run-call: Cu8_0::t_ret_to_uint128 => 27
//@ run-call: Cu8_0::t_param_to_uint128 => 27
//@ run-call: Cu8_0::t_copy_to_uint128 => 27
//@ run-call: Cu8_0::t_ternary_to_uint128 => 27
//@ run-call: Cu8_0::t_ifjoin_to_uint128 => 27
//@ run-call: Cu8_0::t_loopjoin_to_uint128 => 27
//@ run-call: Cu8_0::t_tuple_to_uint128 => 27
//@ run-call: Cu8_0::t_modifier_to_uint128 => 27
//@ run-call: Cu8_0::t_ptr_to_uint128 => 27
//@ run-call: Cu8_0::t_mem_to_uint128 => 27
//@ run-call: Cu8_0::t_local_to_uint256 => 27
//@ run-call: Cu8_0::t_retdirect_to_uint256 => 27
//@ run-call: Cu8_0::t_ret_to_uint256 => 27
//@ run-call: Cu8_0::t_param_to_uint256 => 27
//@ run-call: Cu8_1::t_copy_to_uint256 => 27
//@ run-call: Cu8_1::t_ternary_to_uint256 => 27
//@ run-call: Cu8_1::t_ifjoin_to_uint256 => 27
//@ run-call: Cu8_1::t_loopjoin_to_uint256 => 27
//@ run-call: Cu8_1::t_tuple_to_uint256 => 27
//@ run-call: Cu8_1::t_modifier_to_uint256 => 27
//@ run-call: Cu8_1::t_ptr_to_uint256 => 27
//@ run-call: Cu8_1::t_mem_to_uint256 => 27
//@ run-call: Cu8_1::t_local_to_int_same => 27
//@ run-call: Cu8_1::t_retdirect_to_int_same => 27
//@ run-call: Cu8_1::t_ret_to_int_same => 27
//@ run-call: Cu8_1::t_param_to_int_same => 27
//@ run-call: Cu8_1::t_copy_to_int_same => 27
//@ run-call: Cu8_1::t_ternary_to_int_same => 27
//@ run-call: Cu8_1::t_ifjoin_to_int_same => 27
//@ run-call: Cu8_1::t_loopjoin_to_int_same => 27
//@ run-call: Cu8_1::t_tuple_to_int_same => 27
//@ run-call: Cu8_1::t_modifier_to_int_same => 27
//@ run-call: Cu8_1::t_ptr_to_int_same => 27
//@ run-call: Cu8_1::t_mem_to_int_same => 27
//@ run-call: Cu8_1::t_local_to_int_wide => 27
//@ run-call: Cu8_1::t_retdirect_to_int_wide => 27
//@ run-call: Cu8_1::t_ret_to_int_wide => 27
//@ run-call: Cu8_1::t_param_to_int_wide => 27
//@ run-call: Cu8_1::t_copy_to_int_wide => 27
//@ run-call: Cu8_1::t_ternary_to_int_wide => 27
//@ run-call: Cu8_1::t_ifjoin_to_int_wide => 27
//@ run-call: Cu8_1::t_loopjoin_to_int_wide => 27
//@ run-call: Cu8_1::t_tuple_to_int_wide => 27
//@ run-call: Cu8_1::t_modifier_to_int_wide => 27
//@ run-call: Cu8_1::t_ptr_to_int_wide => 27
//@ run-call: Cu8_1::t_mem_to_int_wide => 27
//@ run-call: Cu8_1::t_local_to_bytes => 27
//@ run-call: Cu8_1::t_retdirect_to_bytes => 27
//@ run-call: Cu8_1::t_ret_to_bytes => 27
//@ run-call: Cu8_1::t_param_to_bytes => 27
//@ run-call: Cu8_1::t_copy_to_bytes => 27
//@ run-call: Cu8_1::t_ternary_to_bytes => 27
//@ run-call: Cu8_1::t_ifjoin_to_bytes => 27
//@ run-call: Cu8_1::t_loopjoin_to_bytes => 27
//@ run-call: Cu8_2::t_tuple_to_bytes => 27
//@ run-call: Cu8_2::t_modifier_to_bytes => 27
//@ run-call: Cu8_2::t_ptr_to_bytes => 27
//@ run-call: Cu8_2::t_mem_to_bytes => 27
//@ run-call: Cu8_2::t_local_to_address => 27
//@ run-call: Cu8_2::t_retdirect_to_address => 27
//@ run-call: Cu8_2::t_ret_to_address => 27
//@ run-call: Cu8_2::t_param_to_address => 27
//@ run-call: Cu8_2::t_copy_to_address => 27
//@ run-call: Cu8_2::t_ternary_to_address => 27
//@ run-call: Cu8_2::t_ifjoin_to_address => 27
//@ run-call: Cu8_2::t_loopjoin_to_address => 27
//@ run-call: Cu8_2::t_tuple_to_address => 27
//@ run-call: Cu8_2::t_modifier_to_address => 27
//@ run-call: Cu8_2::t_ptr_to_address => 27
//@ run-call: Cu8_2::t_mem_to_address => 27
//@ run-call: Cu8_2::t_local_to_enum => 0
//@ run-call: Cu8_2::t_retdirect_to_enum => 0
//@ run-call: Cu8_2::t_ret_to_enum => 0
//@ run-call: Cu8_2::t_param_to_enum => 0
//@ run-call: Cu8_2::t_copy_to_enum => 0
//@ run-call: Cu8_2::t_ternary_to_enum => 0
//@ run-call: Cu8_2::t_ifjoin_to_enum => 0
//@ run-call: Cu8_2::t_loopjoin_to_enum => 0
//@ run-call: Cu8_2::t_tuple_to_enum => 0
//@ run-call: Cu8_2::t_modifier_to_enum => 0
//@ run-call: Cu8_2::t_ptr_to_enum => 0
//@ run-call: Cu8_2::t_mem_to_enum => 0
//@ run-call: Cu8_2::t_local_implicit_widen => 27
//@ run-call: Cu8_2::t_retdirect_implicit_widen => 27
//@ run-call: Cu8_2::t_ret_implicit_widen => 27
//@ run-call: Cu8_2::t_param_implicit_widen => 27
//@ run-call: Cu8_2::t_copy_implicit_widen => 27
//@ run-call: Cu8_2::t_ternary_implicit_widen => 27
//@ run-call: Cu8_2::t_ifjoin_implicit_widen => 27
//@ run-call: Cu8_2::t_loopjoin_implicit_widen => 27
//@ run-call: Cu8_2::t_tuple_implicit_widen => 27
//@ run-call: Cu8_2::t_modifier_implicit_widen => 27
//@ run-call: Cu8_2::t_ptr_implicit_widen => 27
//@ run-call: Cu8_2::t_mem_implicit_widen => 27
//@ run-call: Cu8_3::t_local_narrow_uint8 => 27
//@ run-call: Cu8_3::t_retdirect_narrow_uint8 => 27
//@ run-call: Cu8_3::t_ret_narrow_uint8 => 27
//@ run-call: Cu8_3::t_param_narrow_uint8 => 27
//@ run-call: Cu8_3::t_copy_narrow_uint8 => 27
//@ run-call: Cu8_3::t_ternary_narrow_uint8 => 27
//@ run-call: Cu8_3::t_ifjoin_narrow_uint8 => 27
//@ run-call: Cu8_3::t_loopjoin_narrow_uint8 => 27
//@ run-call: Cu8_3::t_tuple_narrow_uint8 => 27
//@ run-call: Cu8_3::t_modifier_narrow_uint8 => 27
//@ run-call: Cu8_3::t_ptr_narrow_uint8 => 27
//@ run-call: Cu8_3::t_mem_narrow_uint8 => 27
//@ run-call: Cu8_3::t_local_narrow_uint32 => 27
//@ run-call: Cu8_3::t_retdirect_narrow_uint32 => 27
//@ run-call: Cu8_3::t_ret_narrow_uint32 => 27
//@ run-call: Cu8_3::t_param_narrow_uint32 => 27
//@ run-call: Cu8_3::t_copy_narrow_uint32 => 27
//@ run-call: Cu8_3::t_ternary_narrow_uint32 => 27
//@ run-call: Cu8_3::t_ifjoin_narrow_uint32 => 27
//@ run-call: Cu8_3::t_loopjoin_narrow_uint32 => 27
//@ run-call: Cu8_3::t_tuple_narrow_uint32 => 27
//@ run-call: Cu8_3::t_modifier_narrow_uint32 => 27
//@ run-call: Cu8_3::t_ptr_narrow_uint32 => 27
//@ run-call: Cu8_3::t_mem_narrow_uint32 => 27
//@ run-call: Cu64_0::t_local_to_uint8 => 27
//@ run-call: Cu64_0::t_retdirect_to_uint8 => 27
//@ run-call: Cu64_0::t_param_to_uint8 => 27
//@ run-call: Cu64_0::t_mem_to_uint8 => 27
//@ run-call: Cu64_0::t_local_to_uint16 => 27
//@ run-call: Cu64_0::t_retdirect_to_uint16 => 27
//@ run-call: Cu64_0::t_param_to_uint16 => 27
//@ run-call: Cu64_0::t_mem_to_uint16 => 27
//@ run-call: Cu64_0::t_local_to_uint128 => 27
//@ run-call: Cu64_0::t_retdirect_to_uint128 => 27
//@ run-call: Cu64_0::t_param_to_uint128 => 27
//@ run-call: Cu64_0::t_mem_to_uint128 => 27
//@ run-call: Cu64_0::t_local_to_uint256 => 27
//@ run-call: Cu64_0::t_retdirect_to_uint256 => 27
//@ run-call: Cu64_0::t_param_to_uint256 => 27
//@ run-call: Cu64_0::t_mem_to_uint256 => 27
//@ run-call: Cu64_0::t_local_to_int_same => 27
//@ run-call: Cu64_0::t_retdirect_to_int_same => 27
//@ run-call: Cu64_0::t_param_to_int_same => 27
//@ run-call: Cu64_0::t_mem_to_int_same => 27
//@ run-call: Cu64_0::t_local_to_int_wide => 27
//@ run-call: Cu64_0::t_retdirect_to_int_wide => 27
//@ run-call: Cu64_0::t_param_to_int_wide => 27
//@ run-call: Cu64_0::t_mem_to_int_wide => 27
//@ run-call: Cu64_0::t_local_to_bytes => 27
//@ run-call: Cu64_0::t_retdirect_to_bytes => 27
//@ run-call: Cu64_0::t_param_to_bytes => 27
//@ run-call: Cu64_0::t_mem_to_bytes => 27
//@ run-call: Cu64_0::t_local_to_address => 27
//@ run-call: Cu64_0::t_retdirect_to_address => 27
//@ run-call: Cu64_0::t_param_to_address => 27
//@ run-call: Cu64_0::t_mem_to_address => 27
//@ run-call: Cu64_0::t_local_to_enum => 0
//@ run-call: Cu64_0::t_retdirect_to_enum => 0
//@ run-call: Cu64_0::t_param_to_enum => 0
//@ run-call: Cu64_0::t_mem_to_enum => 0
//@ run-call: Cu64_0::t_local_implicit_widen => 27
//@ run-call: Cu64_0::t_retdirect_implicit_widen => 27
//@ run-call: Cu64_0::t_param_implicit_widen => 27
//@ run-call: Cu64_0::t_mem_implicit_widen => 27
//@ run-call: Cu64_1::t_local_narrow_uint8 => 27
//@ run-call: Cu64_1::t_retdirect_narrow_uint8 => 27
//@ run-call: Cu64_1::t_param_narrow_uint8 => 27
//@ run-call: Cu64_1::t_mem_narrow_uint8 => 27
//@ run-call: Cu64_1::t_local_narrow_uint32 => 27
//@ run-call: Cu64_1::t_retdirect_narrow_uint32 => 27
//@ run-call: Cu64_1::t_param_narrow_uint32 => 27
//@ run-call: Cu64_1::t_mem_narrow_uint32 => 27
//@ run-call: Ci8_0::t_local_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_to_uint_same => 253
//@ run-call: Ci8_0::t_retdirect_to_uint_same => 253
//@ run-call: Ci8_0::t_param_to_uint_same => 253
//@ run-call: Ci8_0::t_mem_to_uint_same => 253
//@ run-call: Ci8_0::t_local_to_uint_wide => 65533
//@ run-call: Ci8_0::t_retdirect_to_uint_wide => 65533
//@ run-call: Ci8_0::t_param_to_uint_wide => 65533
//@ run-call: Ci8_0::t_mem_to_uint_wide => 65533
//@ run-call: Ci8_0::t_local_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_int8 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_int8 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_int8 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_int8 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_uint_same => 16777213
//@ run-call: Ci24_0::t_retdirect_to_uint_same => 16777213
//@ run-call: Ci24_0::t_param_to_uint_same => 16777213
//@ run-call: Ci24_0::t_mem_to_uint_same => 16777213
//@ run-call: Ci24_0::t_local_to_uint_wide => 281474976710653
//@ run-call: Ci24_0::t_retdirect_to_uint_wide => 281474976710653
//@ run-call: Ci24_0::t_param_to_uint_wide => 281474976710653
//@ run-call: Ci24_0::t_mem_to_uint_wide => 281474976710653
//@ run-call: Ci24_0::t_local_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Caddr_0::t_local_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ret_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_copy_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ternary_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ifjoin_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_loopjoin_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_tuple_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_modifier_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ptr_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ret_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_copy_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ternary_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ifjoin_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_loopjoin_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_tuple_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_modifier_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ptr_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ret_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_copy_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ternary_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ifjoin_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_loopjoin_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_tuple_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_modifier_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ptr_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ret_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_copy_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_ternary_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_ifjoin_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_loopjoin_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_tuple_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_modifier_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_ptr_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_mem_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_local_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_retdirect_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_ret_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_param_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_copy_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_ternary_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_ifjoin_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_loopjoin_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_tuple_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_modifier_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_ptr_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_mem_to_uint64 => 6230888492328974480
//@ run-call: Caddr_1::t_local_to_uint8 => 144
//@ run-call: Caddr_1::t_retdirect_to_uint8 => 144
//@ run-call: Caddr_1::t_ret_to_uint8 => 144
//@ run-call: Caddr_1::t_param_to_uint8 => 144
//@ run-call: Caddr_1::t_copy_to_uint8 => 144
//@ run-call: Caddr_1::t_ternary_to_uint8 => 144
//@ run-call: Caddr_1::t_ifjoin_to_uint8 => 144
//@ run-call: Caddr_1::t_loopjoin_to_uint8 => 144
//@ run-call: Caddr_1::t_tuple_to_uint8 => 144
//@ run-call: Caddr_1::t_modifier_to_uint8 => 144
//@ run-call: Caddr_1::t_ptr_to_uint8 => 144
//@ run-call: Caddr_1::t_mem_to_uint8 => 144
//@ run-call: Caddr_1::t_local_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_retdirect_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_ret_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_param_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_copy_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_ternary_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_ifjoin_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_loopjoin_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_2::t_tuple_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_2::t_modifier_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_2::t_ptr_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_2::t_mem_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_to_uint64 => 6230888492328974480
//@ run-call: Caddrp_0::t_retdirect_to_uint64 => 6230888492328974480
//@ run-call: Caddrp_0::t_param_to_uint64 => 6230888492328974480
//@ run-call: Caddrp_0::t_mem_to_uint64 => 6230888492328974480
//@ run-call: Caddrp_0::t_local_to_uint8 => 144
//@ run-call: Caddrp_0::t_retdirect_to_uint8 => 144
//@ run-call: Caddrp_0::t_param_to_uint8 => 144
//@ run-call: Caddrp_0::t_mem_to_uint8 => 144
//@ run-call: Cctr_0::t_local_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_to_uint64 => 6230888492328974480
//@ run-call: Cctr_0::t_retdirect_to_uint64 => 6230888492328974480
//@ run-call: Cctr_0::t_param_to_uint64 => 6230888492328974480
//@ run-call: Cctr_0::t_mem_to_uint64 => 6230888492328974480
//@ run-call: Cctr_0::t_local_to_uint8 => 144
//@ run-call: Cctr_0::t_retdirect_to_uint8 => 144
//@ run-call: Cctr_0::t_param_to_uint8 => 144
//@ run-call: Cctr_0::t_mem_to_uint8 => 144
//@ run-call: Cctr_0::t_local_to_ic_addr => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_to_ic_addr => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_to_ic_addr => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_to_ic_addr => 103929005307130220006098923584552504982110632080
//@ run-call: Cbool_0::t_local_to_uint_via_ternary => 1
//@ run-call: Cbool_0::t_retdirect_to_uint_via_ternary => 1
//@ run-call: Cbool_0::t_param_to_uint_via_ternary => 1
//@ run-call: Cbool_0::t_mem_to_uint_via_ternary => 1
//@ run-call: Cbool_0::t_local_to_uint_via_bytes => 1
//@ run-call: Cbool_0::t_retdirect_to_uint_via_bytes => 1
//@ run-call: Cbool_0::t_param_to_uint_via_bytes => 1
//@ run-call: Cbool_0::t_mem_to_uint_via_bytes => 1
//@ run-call: Cb1_0::t_local_to_bytes2 => 43776
//@ run-call: Cb1_0::t_retdirect_to_bytes2 => 43776
//@ run-call: Cb1_0::t_param_to_bytes2 => 43776
//@ run-call: Cb1_0::t_mem_to_bytes2 => 43776
//@ run-call: Cb1_0::t_local_to_bytes4 => 2868903936
//@ run-call: Cb1_0::t_retdirect_to_bytes4 => 2868903936
//@ run-call: Cb1_0::t_param_to_bytes4 => 2868903936
//@ run-call: Cb1_0::t_mem_to_bytes4 => 2868903936
//@ run-call: Cb1_0::t_local_to_bytes8 => 12321848580485677056
//@ run-call: Cb1_0::t_retdirect_to_bytes8 => 12321848580485677056
//@ run-call: Cb1_0::t_param_to_bytes8 => 12321848580485677056
//@ run-call: Cb1_0::t_mem_to_bytes8 => 12321848580485677056
//@ run-call: Cb1_0::t_local_to_bytes16 => 227297987279220614266551007307938922496
//@ run-call: Cb1_0::t_retdirect_to_bytes16 => 227297987279220614266551007307938922496
//@ run-call: Cb1_0::t_param_to_bytes16 => 227297987279220614266551007307938922496
//@ run-call: Cb1_0::t_mem_to_bytes16 => 227297987279220614266551007307938922496
//@ run-call: Cb1_0::t_local_to_bytes20 => 976237421810876558643867603103454673285798690816
//@ run-call: Cb1_0::t_retdirect_to_bytes20 => 976237421810876558643867603103454673285798690816
//@ run-call: Cb1_0::t_param_to_bytes20 => 976237421810876558643867603103454673285798690816
//@ run-call: Cb1_0::t_mem_to_bytes20 => 976237421810876558643867603103454673285798690816
//@ run-call: Cb1_0::t_local_to_bytes32 => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_to_bytes32 => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_to_bytes32 => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_to_bytes32 => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_to_uint => 171
//@ run-call: Cb1_0::t_retdirect_to_uint => 171
//@ run-call: Cb1_0::t_param_to_uint => 171
//@ run-call: Cb1_0::t_mem_to_uint => 171
//@ run-call: Cb1_0::t_local_to_uint_wide => 171
//@ run-call: Cb1_0::t_retdirect_to_uint_wide => 171
//@ run-call: Cb1_0::t_param_to_uint_wide => 171
//@ run-call: Cb1_0::t_mem_to_uint_wide => 171
//@ run-call: Cb1_0::t_local_to_bytes_shift_back => 171
//@ run-call: Cb1_0::t_retdirect_to_bytes_shift_back => 171
//@ run-call: Cb1_0::t_param_to_bytes_shift_back => 171
//@ run-call: Cb1_0::t_mem_to_bytes_shift_back => 171
//@ run-call: Cb4_0::t_local_to_bytes1 => 222
//@ run-call: Cb4_0::t_retdirect_to_bytes1 => 222
//@ run-call: Cb4_0::t_ret_to_bytes1 => 222
//@ run-call: Cb4_0::t_param_to_bytes1 => 222
//@ run-call: Cb4_0::t_copy_to_bytes1 => 222
//@ run-call: Cb4_0::t_ternary_to_bytes1 => 222
//@ run-call: Cb4_0::t_ifjoin_to_bytes1 => 222
//@ run-call: Cb4_0::t_loopjoin_to_bytes1 => 222
//@ run-call: Cb4_0::t_tuple_to_bytes1 => 222
//@ run-call: Cb4_0::t_modifier_to_bytes1 => 222
//@ run-call: Cb4_0::t_ptr_to_bytes1 => 222
//@ run-call: Cb4_0::t_mem_to_bytes1 => 222
//@ run-call: Cb4_0::t_local_to_bytes2 => 57005
//@ run-call: Cb4_0::t_retdirect_to_bytes2 => 57005
//@ run-call: Cb4_0::t_ret_to_bytes2 => 57005
//@ run-call: Cb4_0::t_param_to_bytes2 => 57005
//@ run-call: Cb4_0::t_copy_to_bytes2 => 57005
//@ run-call: Cb4_0::t_ternary_to_bytes2 => 57005
//@ run-call: Cb4_0::t_ifjoin_to_bytes2 => 57005
//@ run-call: Cb4_0::t_loopjoin_to_bytes2 => 57005
//@ run-call: Cb4_0::t_tuple_to_bytes2 => 57005
//@ run-call: Cb4_0::t_modifier_to_bytes2 => 57005
//@ run-call: Cb4_0::t_ptr_to_bytes2 => 57005
//@ run-call: Cb4_0::t_mem_to_bytes2 => 57005
//@ run-call: Cb4_0::t_local_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_retdirect_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_ret_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_param_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_copy_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_ternary_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_ifjoin_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_loopjoin_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_tuple_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_modifier_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_ptr_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_mem_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_local_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_0::t_retdirect_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_0::t_ret_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_0::t_param_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_copy_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_ternary_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_ifjoin_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_loopjoin_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_tuple_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_modifier_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_ptr_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_mem_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_local_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_retdirect_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_ret_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_param_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_copy_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_ternary_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_ifjoin_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_loopjoin_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_tuple_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_modifier_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_ptr_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_mem_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_local_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_retdirect_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_ret_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_param_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_copy_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_ternary_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_ifjoin_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_loopjoin_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_tuple_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_modifier_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_ptr_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_mem_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_local_to_uint => 3735928559
//@ run-call: Cb4_1::t_retdirect_to_uint => 3735928559
//@ run-call: Cb4_1::t_ret_to_uint => 3735928559
//@ run-call: Cb4_1::t_param_to_uint => 3735928559
//@ run-call: Cb4_1::t_copy_to_uint => 3735928559
//@ run-call: Cb4_1::t_ternary_to_uint => 3735928559
//@ run-call: Cb4_1::t_ifjoin_to_uint => 3735928559
//@ run-call: Cb4_1::t_loopjoin_to_uint => 3735928559
//@ run-call: Cb4_2::t_tuple_to_uint => 3735928559
//@ run-call: Cb4_2::t_modifier_to_uint => 3735928559
//@ run-call: Cb4_2::t_ptr_to_uint => 3735928559
//@ run-call: Cb4_2::t_mem_to_uint => 3735928559
//@ run-call: Cb4_2::t_local_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_retdirect_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_ret_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_param_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_copy_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_ternary_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_ifjoin_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_loopjoin_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_tuple_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_modifier_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_ptr_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_mem_to_uint_wide => 3735928559
//@ run-call: Cb4_2::t_local_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_retdirect_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_ret_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_param_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_copy_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_ternary_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_ifjoin_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_loopjoin_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_tuple_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_modifier_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_ptr_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_mem_to_bytes_shift_back => 3735928559
//@ run-call: Cb20_0::t_local_to_bytes1 => 18
//@ run-call: Cb20_0::t_retdirect_to_bytes1 => 18
//@ run-call: Cb20_0::t_param_to_bytes1 => 18
//@ run-call: Cb20_0::t_mem_to_bytes1 => 18
//@ run-call: Cb20_0::t_local_to_bytes2 => 4660
//@ run-call: Cb20_0::t_retdirect_to_bytes2 => 4660
//@ run-call: Cb20_0::t_param_to_bytes2 => 4660
//@ run-call: Cb20_0::t_mem_to_bytes2 => 4660
//@ run-call: Cb20_0::t_local_to_bytes4 => 305419896
//@ run-call: Cb20_0::t_retdirect_to_bytes4 => 305419896
//@ run-call: Cb20_0::t_param_to_bytes4 => 305419896
//@ run-call: Cb20_0::t_mem_to_bytes4 => 305419896
//@ run-call: Cb20_0::t_local_to_bytes8 => 1311768467284833366
//@ run-call: Cb20_0::t_retdirect_to_bytes8 => 1311768467284833366
//@ run-call: Cb20_0::t_param_to_bytes8 => 1311768467284833366
//@ run-call: Cb20_0::t_mem_to_bytes8 => 1311768467284833366
//@ run-call: Cb20_0::t_local_to_bytes16 => 24197857199965561741520400062332047378
//@ run-call: Cb20_0::t_retdirect_to_bytes16 => 24197857199965561741520400062332047378
//@ run-call: Cb20_0::t_param_to_bytes16 => 24197857199965561741520400062332047378
//@ run-call: Cb20_0::t_mem_to_bytes16 => 24197857199965561741520400062332047378
//@ run-call: Cb20_0::t_local_to_bytes32 => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_to_bytes32 => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_to_bytes32 => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_to_bytes32 => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_to_uint => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_to_uint => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_to_uint => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_to_uint => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_local_to_uint_wide => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_to_uint_wide => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_to_uint_wide => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_to_uint_wide => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_local_to_bytes_shift_back => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_to_bytes_shift_back => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_to_bytes_shift_back => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_to_bytes_shift_back => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_local_to_address => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_to_address => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_to_address => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_to_address => 103929005307130220006098923584552504982110632080
//@ run-call-fail: Cen_0::t_local_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_copy_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_ternary_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_ifjoin_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_loopjoin_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_tuple_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_modifier_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_ptr_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_mem_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_unwrap_wide => 27
//@ run-call: Cud_0::t_retdirect_unwrap_wide => 27
//@ run-call: Cud_0::t_param_unwrap_wide => 27
//@ run-call: Cud_0::t_mem_unwrap_wide => 27
//@ run-call: Cud_0::t_local_unwrap_u256 => 27
//@ run-call: Cud_0::t_retdirect_unwrap_u256 => 27
//@ run-call: Cud_0::t_param_unwrap_u256 => 27
//@ run-call: Cud_0::t_mem_unwrap_u256 => 27
//@ run-call: Cud_0::t_local_rewrap => 27
//@ run-call: Cud_0::t_retdirect_rewrap => 27
//@ run-call: Cud_0::t_param_rewrap => 27
//@ run-call: Cud_0::t_mem_rewrap => 27
//@ run-call: Cud_0::t_local_unwrap_to_bytes => 27
//@ run-call: Cud_0::t_retdirect_unwrap_to_bytes => 27
//@ run-call: Cud_0::t_param_unwrap_to_bytes => 27
//@ run-call: Cud_0::t_mem_unwrap_to_bytes => 27

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: conversions of dirty values.

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

interface IF_u8 {
    function g(uint8) external;
}

error Err_u8(uint8 v);
error Err2_u8(uint8 a, uint8 b);
error ErrS_u8(St_u8 s);

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

contract Ctor_u8 {
    uint256 public got;

    constructor(uint8 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_u64 {
    uint256 a;
    uint64 f;
}

interface IF_u64 {
    function g(uint64) external;
}

error Err_u64(uint64 v);
error Err2_u64(uint64 a, uint64 b);
error ErrS_u64(St_u64 s);

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

contract Ctor_u64 {
    uint256 public got;

    constructor(uint64 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_i8 {
    uint256 a;
    int8 f;
}

interface IF_i8 {
    function g(int8) external;
}

error Err_i8(int8 v);
error Err2_i8(int8 a, int8 b);
error ErrS_i8(St_i8 s);

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

contract Ctor_i8 {
    uint256 public got;

    constructor(int8 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_i24 {
    uint256 a;
    int24 f;
}

interface IF_i24 {
    function g(int24) external;
}

error Err_i24(int24 v);
error Err2_i24(int24 a, int24 b);
error ErrS_i24(St_i24 s);

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

contract Ctor_i24 {
    uint256 public got;

    constructor(int24 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_addr {
    uint256 a;
    address f;
}

interface IF_addr {
    function g(address) external;
}

error Err_addr(address v);
error Err2_addr(address a, address b);
error ErrS_addr(St_addr s);

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

contract Ctor_addr {
    uint256 public got;

    constructor(address v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_addrp {
    uint256 a;
    address payable f;
}

interface IF_addrp {
    function g(address payable) external;
}

error Err_addrp(address payable v);
error Err2_addrp(address payable a, address payable b);
error ErrS_addrp(St_addrp s);

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

contract Ctor_addrp {
    uint256 public got;

    constructor(address payable v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_ctr {
    uint256 a;
    IC f;
}

interface IF_ctr {
    function g(IC) external;
}

error Err_ctr(IC v);
error Err2_ctr(IC a, IC b);
error ErrS_ctr(St_ctr s);

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

contract Ctor_ctr {
    uint256 public got;

    constructor(IC v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_bool {
    uint256 a;
    bool f;
}

interface IF_bool {
    function g(bool) external;
}

error Err_bool(bool v);
error Err2_bool(bool a, bool b);
error ErrS_bool(St_bool s);

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

contract Ctor_bool {
    uint256 public got;

    constructor(bool v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_b1 {
    uint256 a;
    bytes1 f;
}

interface IF_b1 {
    function g(bytes1) external;
}

error Err_b1(bytes1 v);
error Err2_b1(bytes1 a, bytes1 b);
error ErrS_b1(St_b1 s);

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

contract Ctor_b1 {
    uint256 public got;

    constructor(bytes1 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_b4 {
    uint256 a;
    bytes4 f;
}

interface IF_b4 {
    function g(bytes4) external;
}

error Err_b4(bytes4 v);
error Err2_b4(bytes4 a, bytes4 b);
error ErrS_b4(St_b4 s);

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

contract Ctor_b4 {
    uint256 public got;

    constructor(bytes4 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_b20 {
    uint256 a;
    bytes20 f;
}

interface IF_b20 {
    function g(bytes20) external;
}

error Err_b20(bytes20 v);
error Err2_b20(bytes20 a, bytes20 b);
error ErrS_b20(St_b20 s);

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

contract Ctor_b20 {
    uint256 public got;

    constructor(bytes20 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_en {
    uint256 a;
    E f;
}

interface IF_en {
    function g(E) external;
}

error Err_en(E v);
error Err2_en(E a, E b);
error ErrS_en(St_en s);

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

contract Ctor_en {
    uint256 public got;

    constructor(E v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_ud {
    uint256 a;
    U f;
}

interface IF_ud {
    function g(U) external;
}

error Err_ud(U v);
error Err2_ud(U a, U b);
error ErrS_ud(St_ud s);

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

contract Ctor_ud {
    uint256 public got;

    constructor(U v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Cu8_0 {
    function _s_retdirect_to_uint16() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_uint16() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_uint16(uint8 pv) internal view returns (uint256) { return uint256(uint16(pv)); }
    uint256 _res_modifier_to_uint16;
    modifier _m_modifier_to_uint16(uint8 pv) { _res_modifier_to_uint16 = uint256(uint16(pv)); _; }
    function _e_modifier_to_uint16(uint8 rd) internal _m_modifier_to_uint16(rd) returns (uint256) { return _res_modifier_to_uint16; }
    function _w_ptr_to_uint16() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_uint64() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_uint64() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_uint64(uint8 pv) internal view returns (uint256) { return uint256(uint64(pv)); }
    uint256 _res_modifier_to_uint64;
    modifier _m_modifier_to_uint64(uint8 pv) { _res_modifier_to_uint64 = uint256(uint64(pv)); _; }
    function _e_modifier_to_uint64(uint8 rd) internal _m_modifier_to_uint64(rd) returns (uint256) { return _res_modifier_to_uint64; }
    function _w_ptr_to_uint64() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_uint128() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_uint128() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_uint128(uint8 pv) internal view returns (uint256) { return uint256(uint128(pv)); }
    uint256 _res_modifier_to_uint128;
    modifier _m_modifier_to_uint128(uint8 pv) { _res_modifier_to_uint128 = uint256(uint128(pv)); _; }
    function _e_modifier_to_uint128(uint8 rd) internal _m_modifier_to_uint128(rd) returns (uint256) { return _res_modifier_to_uint128; }
    function _w_ptr_to_uint128() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_uint256() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_uint256() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_uint256(uint8 pv) internal view returns (uint256) { return uint256(uint256(pv)); }
    function t_local_to_uint16() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint16(rx)); }
    function t_retdirect_to_uint16() external view returns (uint256) { return uint256(uint16(_s_retdirect_to_uint16())); }
    function t_ret_to_uint16() external view returns (uint256) { uint8 rx = _s_ret_to_uint16(); return uint256(uint16(rx)); }
    function t_param_to_uint16() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_uint16(rx); }
    function t_copy_to_uint16() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint16(ry)); }
    function t_ternary_to_uint16() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint16(ry)); }
    function t_ifjoin_to_uint16() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint16(ry)); }
    function t_loopjoin_to_uint16() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint16(ry)); }
    function t_tuple_to_uint16() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint16(ry)); }
    function t_modifier_to_uint16() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_uint16(rx); }
    function t_ptr_to_uint16() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint16; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint16(ry)); }
    function t_mem_to_uint16() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint16(ry)); }
    function t_local_to_uint64() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint64(rx)); }
    function t_retdirect_to_uint64() external view returns (uint256) { return uint256(uint64(_s_retdirect_to_uint64())); }
    function t_ret_to_uint64() external view returns (uint256) { uint8 rx = _s_ret_to_uint64(); return uint256(uint64(rx)); }
    function t_param_to_uint64() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_uint64(rx); }
    function t_copy_to_uint64() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint64(ry)); }
    function t_ternary_to_uint64() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint64(ry)); }
    function t_ifjoin_to_uint64() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint64(ry)); }
    function t_loopjoin_to_uint64() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint64(ry)); }
    function t_tuple_to_uint64() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint64(ry)); }
    function t_modifier_to_uint64() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_uint64(rx); }
    function t_ptr_to_uint64() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint64; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint64(ry)); }
    function t_mem_to_uint64() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint64(ry)); }
    function t_local_to_uint128() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint128(rx)); }
    function t_retdirect_to_uint128() external view returns (uint256) { return uint256(uint128(_s_retdirect_to_uint128())); }
    function t_ret_to_uint128() external view returns (uint256) { uint8 rx = _s_ret_to_uint128(); return uint256(uint128(rx)); }
    function t_param_to_uint128() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_uint128(rx); }
    function t_copy_to_uint128() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint128(ry)); }
    function t_ternary_to_uint128() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint128(ry)); }
    function t_ifjoin_to_uint128() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint128(ry)); }
    function t_loopjoin_to_uint128() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint128(ry)); }
    function t_tuple_to_uint128() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint128(ry)); }
    function t_modifier_to_uint128() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_uint128(rx); }
    function t_ptr_to_uint128() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint128; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint128(ry)); }
    function t_mem_to_uint128() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint128(ry)); }
    function t_local_to_uint256() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint256(rx)); }
    function t_retdirect_to_uint256() external view returns (uint256) { return uint256(uint256(_s_retdirect_to_uint256())); }
    function t_ret_to_uint256() external view returns (uint256) { uint8 rx = _s_ret_to_uint256(); return uint256(uint256(rx)); }
    function t_param_to_uint256() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_uint256(rx); }
}
contract Cu8_1 {
    uint256 _res_modifier_to_uint256;
    modifier _m_modifier_to_uint256(uint8 pv) { _res_modifier_to_uint256 = uint256(uint256(pv)); _; }
    function _e_modifier_to_uint256(uint8 rd) internal _m_modifier_to_uint256(rd) returns (uint256) { return _res_modifier_to_uint256; }
    function _w_ptr_to_uint256() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_int_same() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_int_same() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_int_same(uint8 pv) internal view returns (uint256) { return uint256(uint8(int8(pv))); }
    uint256 _res_modifier_to_int_same;
    modifier _m_modifier_to_int_same(uint8 pv) { _res_modifier_to_int_same = uint256(uint8(int8(pv))); _; }
    function _e_modifier_to_int_same(uint8 rd) internal _m_modifier_to_int_same(rd) returns (uint256) { return _res_modifier_to_int_same; }
    function _w_ptr_to_int_same() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_int_wide() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_int_wide() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_int_wide(uint8 pv) internal view returns (uint256) { return uint256(int256(int16(uint16(pv)))); }
    uint256 _res_modifier_to_int_wide;
    modifier _m_modifier_to_int_wide(uint8 pv) { _res_modifier_to_int_wide = uint256(int256(int16(uint16(pv)))); _; }
    function _e_modifier_to_int_wide(uint8 rd) internal _m_modifier_to_int_wide(rd) returns (uint256) { return _res_modifier_to_int_wide; }
    function _w_ptr_to_int_wide() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_bytes() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_bytes() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_bytes(uint8 pv) internal view returns (uint256) { return uint256(uint8(bytes1(pv))); }
    function t_copy_to_uint256() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint256(ry)); }
    function t_ternary_to_uint256() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint256(ry)); }
    function t_ifjoin_to_uint256() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint256(ry)); }
    function t_loopjoin_to_uint256() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint256(ry)); }
    function t_tuple_to_uint256() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint256(ry)); }
    function t_modifier_to_uint256() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_uint256(rx); }
    function t_ptr_to_uint256() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint256; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint256(ry)); }
    function t_mem_to_uint256() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint256(ry)); }
    function t_local_to_int_same() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(int8(rx))); }
    function t_retdirect_to_int_same() external view returns (uint256) { return uint256(uint8(int8(_s_retdirect_to_int_same()))); }
    function t_ret_to_int_same() external view returns (uint256) { uint8 rx = _s_ret_to_int_same(); return uint256(uint8(int8(rx))); }
    function t_param_to_int_same() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_int_same(rx); }
    function t_copy_to_int_same() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(int8(ry))); }
    function t_ternary_to_int_same() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(int8(ry))); }
    function t_ifjoin_to_int_same() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(int8(ry))); }
    function t_loopjoin_to_int_same() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(int8(ry))); }
    function t_tuple_to_int_same() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(int8(ry))); }
    function t_modifier_to_int_same() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_int_same(rx); }
    function t_ptr_to_int_same() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_int_same; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(int8(ry))); }
    function t_mem_to_int_same() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(int8(ry))); }
    function t_local_to_int_wide() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(int256(int16(uint16(rx)))); }
    function t_retdirect_to_int_wide() external view returns (uint256) { return uint256(int256(int16(uint16(_s_retdirect_to_int_wide())))); }
    function t_ret_to_int_wide() external view returns (uint256) { uint8 rx = _s_ret_to_int_wide(); return uint256(int256(int16(uint16(rx)))); }
    function t_param_to_int_wide() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_int_wide(rx); }
    function t_copy_to_int_wide() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(int256(int16(uint16(ry)))); }
    function t_ternary_to_int_wide() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(int256(int16(uint16(ry)))); }
    function t_ifjoin_to_int_wide() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(int256(int16(uint16(ry)))); }
    function t_loopjoin_to_int_wide() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(int256(int16(uint16(ry)))); }
    function t_tuple_to_int_wide() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(int256(int16(uint16(ry)))); }
    function t_modifier_to_int_wide() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_int_wide(rx); }
    function t_ptr_to_int_wide() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_int_wide; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(int256(int16(uint16(ry)))); }
    function t_mem_to_int_wide() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(int256(int16(uint16(ry)))); }
    function t_local_to_bytes() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(bytes1(rx))); }
    function t_retdirect_to_bytes() external view returns (uint256) { return uint256(uint8(bytes1(_s_retdirect_to_bytes()))); }
    function t_ret_to_bytes() external view returns (uint256) { uint8 rx = _s_ret_to_bytes(); return uint256(uint8(bytes1(rx))); }
    function t_param_to_bytes() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_bytes(rx); }
    function t_copy_to_bytes() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(bytes1(ry))); }
    function t_ternary_to_bytes() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(bytes1(ry))); }
    function t_ifjoin_to_bytes() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(bytes1(ry))); }
    function t_loopjoin_to_bytes() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(bytes1(ry))); }
}
contract Cu8_2 {
    uint256 _res_modifier_to_bytes;
    modifier _m_modifier_to_bytes(uint8 pv) { _res_modifier_to_bytes = uint256(uint8(bytes1(pv))); _; }
    function _e_modifier_to_bytes(uint8 rd) internal _m_modifier_to_bytes(rd) returns (uint256) { return _res_modifier_to_bytes; }
    function _w_ptr_to_bytes() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_address() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_address() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_address(uint8 pv) internal view returns (uint256) { return uint256(uint160(address(uint160(pv)))); }
    uint256 _res_modifier_to_address;
    modifier _m_modifier_to_address(uint8 pv) { _res_modifier_to_address = uint256(uint160(address(uint160(pv)))); _; }
    function _e_modifier_to_address(uint8 rd) internal _m_modifier_to_address(rd) returns (uint256) { return _res_modifier_to_address; }
    function _w_ptr_to_address() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_enum() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_enum() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_enum(uint8 pv) internal view returns (uint256) { return uint256(uint8(E(uint8(pv % 3)))); }
    uint256 _res_modifier_to_enum;
    modifier _m_modifier_to_enum(uint8 pv) { _res_modifier_to_enum = uint256(uint8(E(uint8(pv % 3)))); _; }
    function _e_modifier_to_enum(uint8 rd) internal _m_modifier_to_enum(rd) returns (uint256) { return _res_modifier_to_enum; }
    function _w_ptr_to_enum() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_implicit_widen() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_implicit_widen() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_implicit_widen(uint8 pv) internal view returns (uint256) { return uint256(pv) + 0; }
    uint256 _res_modifier_implicit_widen;
    modifier _m_modifier_implicit_widen(uint8 pv) { _res_modifier_implicit_widen = uint256(pv) + 0; _; }
    function _e_modifier_implicit_widen(uint8 rd) internal _m_modifier_implicit_widen(rd) returns (uint256) { return _res_modifier_implicit_widen; }
    function _w_ptr_implicit_widen() internal pure returns (uint256) { return 0xabcd1b; }
    function t_tuple_to_bytes() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(bytes1(ry))); }
    function t_modifier_to_bytes() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_bytes(rx); }
    function t_ptr_to_bytes() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(bytes1(ry))); }
    function t_mem_to_bytes() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(bytes1(ry))); }
    function t_local_to_address() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint160(address(uint160(rx)))); }
    function t_retdirect_to_address() external view returns (uint256) { return uint256(uint160(address(uint160(_s_retdirect_to_address())))); }
    function t_ret_to_address() external view returns (uint256) { uint8 rx = _s_ret_to_address(); return uint256(uint160(address(uint160(rx)))); }
    function t_param_to_address() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_address(rx); }
    function t_copy_to_address() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint160(address(uint160(ry)))); }
    function t_ternary_to_address() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint160(address(uint160(ry)))); }
    function t_ifjoin_to_address() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(address(uint160(ry)))); }
    function t_loopjoin_to_address() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(address(uint160(ry)))); }
    function t_tuple_to_address() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(address(uint160(ry)))); }
    function t_modifier_to_address() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_address(rx); }
    function t_ptr_to_address() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_address; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint160(address(uint160(ry)))); }
    function t_mem_to_address() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint160(address(uint160(ry)))); }
    function t_local_to_enum() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(E(uint8(rx % 3)))); }
    function t_retdirect_to_enum() external view returns (uint256) { return uint256(uint8(E(uint8(_s_retdirect_to_enum() % 3)))); }
    function t_ret_to_enum() external view returns (uint256) { uint8 rx = _s_ret_to_enum(); return uint256(uint8(E(uint8(rx % 3)))); }
    function t_param_to_enum() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_enum(rx); }
    function t_copy_to_enum() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(E(uint8(ry % 3)))); }
    function t_ternary_to_enum() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(E(uint8(ry % 3)))); }
    function t_ifjoin_to_enum() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(E(uint8(ry % 3)))); }
    function t_loopjoin_to_enum() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(E(uint8(ry % 3)))); }
    function t_tuple_to_enum() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(E(uint8(ry % 3)))); }
    function t_modifier_to_enum() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_enum(rx); }
    function t_ptr_to_enum() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_enum; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(E(uint8(ry % 3)))); }
    function t_mem_to_enum() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(E(uint8(ry % 3)))); }
    function t_local_implicit_widen() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(rx) + 0; }
    function t_retdirect_implicit_widen() external view returns (uint256) { return uint256(_s_retdirect_implicit_widen()) + 0; }
    function t_ret_implicit_widen() external view returns (uint256) { uint8 rx = _s_ret_implicit_widen(); return uint256(rx) + 0; }
    function t_param_implicit_widen() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_implicit_widen(rx); }
    function t_copy_implicit_widen() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(ry) + 0; }
    function t_ternary_implicit_widen() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(ry) + 0; }
    function t_ifjoin_implicit_widen() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(ry) + 0; }
    function t_loopjoin_implicit_widen() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(ry) + 0; }
    function t_tuple_implicit_widen() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(ry) + 0; }
    function t_modifier_implicit_widen() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_implicit_widen(rx); }
    function t_ptr_implicit_widen() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_implicit_widen; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(ry) + 0; }
    function t_mem_implicit_widen() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(ry) + 0; }
}
contract Cu8_3 {
    function _s_retdirect_narrow_uint8() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_narrow_uint8() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_narrow_uint8(uint8 pv) internal view returns (uint256) { return uint256(uint8(pv)); }
    uint256 _res_modifier_narrow_uint8;
    modifier _m_modifier_narrow_uint8(uint8 pv) { _res_modifier_narrow_uint8 = uint256(uint8(pv)); _; }
    function _e_modifier_narrow_uint8(uint8 rd) internal _m_modifier_narrow_uint8(rd) returns (uint256) { return _res_modifier_narrow_uint8; }
    function _w_ptr_narrow_uint8() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_narrow_uint32() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_narrow_uint32() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_narrow_uint32(uint8 pv) internal view returns (uint256) { return uint256(uint32(pv)); }
    uint256 _res_modifier_narrow_uint32;
    modifier _m_modifier_narrow_uint32(uint8 pv) { _res_modifier_narrow_uint32 = uint256(uint32(pv)); _; }
    function _e_modifier_narrow_uint32(uint8 rd) internal _m_modifier_narrow_uint32(rd) returns (uint256) { return _res_modifier_narrow_uint32; }
    function _w_ptr_narrow_uint32() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_narrow_uint8() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(rx)); }
    function t_retdirect_narrow_uint8() external view returns (uint256) { return uint256(uint8(_s_retdirect_narrow_uint8())); }
    function t_ret_narrow_uint8() external view returns (uint256) { uint8 rx = _s_ret_narrow_uint8(); return uint256(uint8(rx)); }
    function t_param_narrow_uint8() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_narrow_uint8(rx); }
    function t_copy_narrow_uint8() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(ry)); }
    function t_ternary_narrow_uint8() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(ry)); }
    function t_ifjoin_narrow_uint8() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(ry)); }
    function t_loopjoin_narrow_uint8() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(ry)); }
    function t_tuple_narrow_uint8() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(ry)); }
    function t_modifier_narrow_uint8() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_narrow_uint8(rx); }
    function t_ptr_narrow_uint8() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_narrow_uint8; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(ry)); }
    function t_mem_narrow_uint8() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_narrow_uint32() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint32(rx)); }
    function t_retdirect_narrow_uint32() external view returns (uint256) { return uint256(uint32(_s_retdirect_narrow_uint32())); }
    function t_ret_narrow_uint32() external view returns (uint256) { uint8 rx = _s_ret_narrow_uint32(); return uint256(uint32(rx)); }
    function t_param_narrow_uint32() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_narrow_uint32(rx); }
    function t_copy_narrow_uint32() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint32(ry)); }
    function t_ternary_narrow_uint32() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint32(ry)); }
    function t_ifjoin_narrow_uint32() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry)); }
    function t_loopjoin_narrow_uint32() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry)); }
    function t_tuple_narrow_uint32() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry)); }
    function t_modifier_narrow_uint32() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_narrow_uint32(rx); }
    function t_ptr_narrow_uint32() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_narrow_uint32; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint32(ry)); }
    function t_mem_narrow_uint32() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint32(ry)); }
}
contract Cu64_0 {
    function _s_retdirect_to_uint8() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_uint8(uint64 pv) internal view returns (uint256) { return uint256(uint8(pv)); }
    function _s_retdirect_to_uint16() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_uint16(uint64 pv) internal view returns (uint256) { return uint256(uint16(pv)); }
    function _s_retdirect_to_uint128() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_uint128(uint64 pv) internal view returns (uint256) { return uint256(uint128(pv)); }
    function _s_retdirect_to_uint256() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_uint256(uint64 pv) internal view returns (uint256) { return uint256(uint256(pv)); }
    function _s_retdirect_to_int_same() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_int_same(uint64 pv) internal view returns (uint256) { return uint256(uint64(int64(pv))); }
    function _s_retdirect_to_int_wide() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_int_wide(uint64 pv) internal view returns (uint256) { return uint256(int256(int128(uint128(pv)))); }
    function _s_retdirect_to_bytes() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_bytes(uint64 pv) internal view returns (uint256) { return uint256(uint64(bytes8(pv))); }
    function _s_retdirect_to_address() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_address(uint64 pv) internal view returns (uint256) { return uint256(uint160(address(uint160(pv)))); }
    function _s_retdirect_to_enum() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_enum(uint64 pv) internal view returns (uint256) { return uint256(uint8(E(uint8(pv % 3)))); }
    function _s_retdirect_implicit_widen() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_implicit_widen(uint64 pv) internal view returns (uint256) { return uint256(pv) + 0; }
    function t_local_to_uint8() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint8(rx)); }
    function t_retdirect_to_uint8() external view returns (uint256) { return uint256(uint8(_s_retdirect_to_uint8())); }
    function t_param_to_uint8() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_uint8(rx); }
    function t_mem_to_uint8() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_to_uint16() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint16(rx)); }
    function t_retdirect_to_uint16() external view returns (uint256) { return uint256(uint16(_s_retdirect_to_uint16())); }
    function t_param_to_uint16() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_uint16(rx); }
    function t_mem_to_uint16() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint16(ry)); }
    function t_local_to_uint128() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint128(rx)); }
    function t_retdirect_to_uint128() external view returns (uint256) { return uint256(uint128(_s_retdirect_to_uint128())); }
    function t_param_to_uint128() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_uint128(rx); }
    function t_mem_to_uint128() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint128(ry)); }
    function t_local_to_uint256() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint256(rx)); }
    function t_retdirect_to_uint256() external view returns (uint256) { return uint256(uint256(_s_retdirect_to_uint256())); }
    function t_param_to_uint256() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_uint256(rx); }
    function t_mem_to_uint256() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint256(ry)); }
    function t_local_to_int_same() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint64(int64(rx))); }
    function t_retdirect_to_int_same() external view returns (uint256) { return uint256(uint64(int64(_s_retdirect_to_int_same()))); }
    function t_param_to_int_same() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_int_same(rx); }
    function t_mem_to_int_same() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint64(int64(ry))); }
    function t_local_to_int_wide() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(int256(int128(uint128(rx)))); }
    function t_retdirect_to_int_wide() external view returns (uint256) { return uint256(int256(int128(uint128(_s_retdirect_to_int_wide())))); }
    function t_param_to_int_wide() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_int_wide(rx); }
    function t_mem_to_int_wide() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(int256(int128(uint128(ry)))); }
    function t_local_to_bytes() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint64(bytes8(rx))); }
    function t_retdirect_to_bytes() external view returns (uint256) { return uint256(uint64(bytes8(_s_retdirect_to_bytes()))); }
    function t_param_to_bytes() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_bytes(rx); }
    function t_mem_to_bytes() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint64(bytes8(ry))); }
    function t_local_to_address() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint160(address(uint160(rx)))); }
    function t_retdirect_to_address() external view returns (uint256) { return uint256(uint160(address(uint160(_s_retdirect_to_address())))); }
    function t_param_to_address() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_address(rx); }
    function t_mem_to_address() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint160(address(uint160(ry)))); }
    function t_local_to_enum() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint8(E(uint8(rx % 3)))); }
    function t_retdirect_to_enum() external view returns (uint256) { return uint256(uint8(E(uint8(_s_retdirect_to_enum() % 3)))); }
    function t_param_to_enum() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_enum(rx); }
    function t_mem_to_enum() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint8(E(uint8(ry % 3)))); }
    function t_local_implicit_widen() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(rx) + 0; }
    function t_retdirect_implicit_widen() external view returns (uint256) { return uint256(_s_retdirect_implicit_widen()) + 0; }
    function t_param_implicit_widen() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_implicit_widen(rx); }
    function t_mem_implicit_widen() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(ry) + 0; }
}
contract Cu64_1 {
    function _s_retdirect_narrow_uint8() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_narrow_uint8(uint64 pv) internal view returns (uint256) { return uint256(uint8(pv)); }
    function _s_retdirect_narrow_uint32() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_narrow_uint32(uint64 pv) internal view returns (uint256) { return uint256(uint32(pv)); }
    function t_local_narrow_uint8() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint8(rx)); }
    function t_retdirect_narrow_uint8() external view returns (uint256) { return uint256(uint8(_s_retdirect_narrow_uint8())); }
    function t_param_narrow_uint8() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_narrow_uint8(rx); }
    function t_mem_narrow_uint8() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_narrow_uint32() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint32(rx)); }
    function t_retdirect_narrow_uint32() external view returns (uint256) { return uint256(uint32(_s_retdirect_narrow_uint32())); }
    function t_param_narrow_uint32() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_narrow_uint32(rx); }
    function t_mem_narrow_uint32() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint32(ry)); }
}
contract Ci8_0 {
    function _s_retdirect_to_int16() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_int16(int8 pv) internal view returns (uint256) { return uint256(int256(int16(pv))); }
    function _s_retdirect_to_int64() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_int64(int8 pv) internal view returns (uint256) { return uint256(int256(int64(pv))); }
    function _s_retdirect_to_int128() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_int128(int8 pv) internal view returns (uint256) { return uint256(int256(int128(pv))); }
    function _s_retdirect_to_int256() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_int256(int8 pv) internal view returns (uint256) { return uint256(int256(int256(pv))); }
    function _s_retdirect_to_uint_same() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_uint_same(int8 pv) internal view returns (uint256) { return uint256(uint8(pv)); }
    function _s_retdirect_to_uint_wide() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_uint_wide(int8 pv) internal view returns (uint256) { return uint256(uint16(int16(pv))); }
    function _s_retdirect_to_uint256() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_uint256(int8 pv) internal view returns (uint256) { return uint256(int256(pv)); }
    function _s_retdirect_to_uint256_wide() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_uint256_wide(int8 pv) internal view returns (uint256) { return uint256(uint256(int256(pv))); }
    function _s_retdirect_implicit_widen() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_implicit_widen(int8 pv) internal view returns (uint256) { return uint256(int256(pv) + 0); }
    function t_local_to_int16() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int16(rx))); }
    function t_retdirect_to_int16() external view returns (uint256) { return uint256(int256(int16(_s_retdirect_to_int16()))); }
    function t_param_to_int16() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_int16(rx); }
    function t_mem_to_int16() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int16(ry))); }
    function t_local_to_int64() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int64(rx))); }
    function t_retdirect_to_int64() external view returns (uint256) { return uint256(int256(int64(_s_retdirect_to_int64()))); }
    function t_param_to_int64() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_int64(rx); }
    function t_mem_to_int64() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int64(ry))); }
    function t_local_to_int128() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int128(rx))); }
    function t_retdirect_to_int128() external view returns (uint256) { return uint256(int256(int128(_s_retdirect_to_int128()))); }
    function t_param_to_int128() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_int128(rx); }
    function t_mem_to_int128() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int128(ry))); }
    function t_local_to_int256() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int256(rx))); }
    function t_retdirect_to_int256() external view returns (uint256) { return uint256(int256(int256(_s_retdirect_to_int256()))); }
    function t_param_to_int256() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_int256(rx); }
    function t_mem_to_int256() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int256(ry))); }
    function t_local_to_uint_same() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(uint8(rx)); }
    function t_retdirect_to_uint_same() external view returns (uint256) { return uint256(uint8(_s_retdirect_to_uint_same())); }
    function t_param_to_uint_same() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_uint_same(rx); }
    function t_mem_to_uint_same() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_to_uint_wide() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(uint16(int16(rx))); }
    function t_retdirect_to_uint_wide() external view returns (uint256) { return uint256(uint16(int16(_s_retdirect_to_uint_wide()))); }
    function t_param_to_uint_wide() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_uint_wide(rx); }
    function t_mem_to_uint_wide() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(uint16(int16(ry))); }
    function t_local_to_uint256() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(rx)); }
    function t_retdirect_to_uint256() external view returns (uint256) { return uint256(int256(_s_retdirect_to_uint256())); }
    function t_param_to_uint256() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_uint256(rx); }
    function t_mem_to_uint256() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(ry)); }
    function t_local_to_uint256_wide() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(uint256(int256(rx))); }
    function t_retdirect_to_uint256_wide() external view returns (uint256) { return uint256(uint256(int256(_s_retdirect_to_uint256_wide()))); }
    function t_param_to_uint256_wide() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_uint256_wide(rx); }
    function t_mem_to_uint256_wide() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(uint256(int256(ry))); }
    function t_local_implicit_widen() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(rx) + 0); }
    function t_retdirect_implicit_widen() external view returns (uint256) { return uint256(int256(_s_retdirect_implicit_widen()) + 0); }
    function t_param_implicit_widen() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_implicit_widen(rx); }
    function t_mem_implicit_widen() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(ry) + 0); }
}
contract Ci24_0 {
    function _s_retdirect_to_int8() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_int8(int24 pv) internal view returns (uint256) { return uint256(int256(int8(pv))); }
    function _s_retdirect_to_int16() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_int16(int24 pv) internal view returns (uint256) { return uint256(int256(int16(pv))); }
    function _s_retdirect_to_int64() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_int64(int24 pv) internal view returns (uint256) { return uint256(int256(int64(pv))); }
    function _s_retdirect_to_int128() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_int128(int24 pv) internal view returns (uint256) { return uint256(int256(int128(pv))); }
    function _s_retdirect_to_int256() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_int256(int24 pv) internal view returns (uint256) { return uint256(int256(int256(pv))); }
    function _s_retdirect_to_uint_same() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_uint_same(int24 pv) internal view returns (uint256) { return uint256(uint24(pv)); }
    function _s_retdirect_to_uint_wide() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_uint_wide(int24 pv) internal view returns (uint256) { return uint256(uint48(int48(pv))); }
    function _s_retdirect_to_uint256() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_uint256(int24 pv) internal view returns (uint256) { return uint256(int256(pv)); }
    function _s_retdirect_to_uint256_wide() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_uint256_wide(int24 pv) internal view returns (uint256) { return uint256(uint256(int256(pv))); }
    function _s_retdirect_implicit_widen() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_implicit_widen(int24 pv) internal view returns (uint256) { return uint256(int256(pv) + 0); }
    function t_local_to_int8() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int8(rx))); }
    function t_retdirect_to_int8() external view returns (uint256) { return uint256(int256(int8(_s_retdirect_to_int8()))); }
    function t_param_to_int8() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_int8(rx); }
    function t_mem_to_int8() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int8(ry))); }
    function t_local_to_int16() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int16(rx))); }
    function t_retdirect_to_int16() external view returns (uint256) { return uint256(int256(int16(_s_retdirect_to_int16()))); }
    function t_param_to_int16() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_int16(rx); }
    function t_mem_to_int16() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int16(ry))); }
    function t_local_to_int64() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int64(rx))); }
    function t_retdirect_to_int64() external view returns (uint256) { return uint256(int256(int64(_s_retdirect_to_int64()))); }
    function t_param_to_int64() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_int64(rx); }
    function t_mem_to_int64() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int64(ry))); }
    function t_local_to_int128() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int128(rx))); }
    function t_retdirect_to_int128() external view returns (uint256) { return uint256(int256(int128(_s_retdirect_to_int128()))); }
    function t_param_to_int128() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_int128(rx); }
    function t_mem_to_int128() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int128(ry))); }
    function t_local_to_int256() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int256(rx))); }
    function t_retdirect_to_int256() external view returns (uint256) { return uint256(int256(int256(_s_retdirect_to_int256()))); }
    function t_param_to_int256() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_int256(rx); }
    function t_mem_to_int256() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int256(ry))); }
    function t_local_to_uint_same() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(uint24(rx)); }
    function t_retdirect_to_uint_same() external view returns (uint256) { return uint256(uint24(_s_retdirect_to_uint_same())); }
    function t_param_to_uint_same() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_uint_same(rx); }
    function t_mem_to_uint_same() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(uint24(ry)); }
    function t_local_to_uint_wide() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(uint48(int48(rx))); }
    function t_retdirect_to_uint_wide() external view returns (uint256) { return uint256(uint48(int48(_s_retdirect_to_uint_wide()))); }
    function t_param_to_uint_wide() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_uint_wide(rx); }
    function t_mem_to_uint_wide() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(uint48(int48(ry))); }
    function t_local_to_uint256() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(rx)); }
    function t_retdirect_to_uint256() external view returns (uint256) { return uint256(int256(_s_retdirect_to_uint256())); }
    function t_param_to_uint256() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_uint256(rx); }
    function t_mem_to_uint256() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(ry)); }
    function t_local_to_uint256_wide() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(uint256(int256(rx))); }
    function t_retdirect_to_uint256_wide() external view returns (uint256) { return uint256(uint256(int256(_s_retdirect_to_uint256_wide()))); }
    function t_param_to_uint256_wide() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_uint256_wide(rx); }
    function t_mem_to_uint256_wide() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(uint256(int256(ry))); }
    function t_local_implicit_widen() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(rx) + 0); }
    function t_retdirect_implicit_widen() external view returns (uint256) { return uint256(int256(_s_retdirect_implicit_widen()) + 0); }
    function t_param_implicit_widen() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_implicit_widen(rx); }
    function t_mem_implicit_widen() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(ry) + 0); }
}
contract Caddr_0 {
    function _s_retdirect_to_uint160() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_uint160() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint160(address pv) internal view returns (uint256) { return uint256(uint160(address(pv))); }
    uint256 _res_modifier_to_uint160;
    modifier _m_modifier_to_uint160(address pv) { _res_modifier_to_uint160 = uint256(uint160(address(pv))); _; }
    function _e_modifier_to_uint160(address rd) internal _m_modifier_to_uint160(rd) returns (uint256) { return _res_modifier_to_uint160; }
    function _w_ptr_to_uint160() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_to_uint256() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_uint256() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint256(address pv) internal view returns (uint256) { return uint256(uint160(address(pv))); }
    uint256 _res_modifier_to_uint256;
    modifier _m_modifier_to_uint256(address pv) { _res_modifier_to_uint256 = uint256(uint160(address(pv))); _; }
    function _e_modifier_to_uint256(address rd) internal _m_modifier_to_uint256(rd) returns (uint256) { return _res_modifier_to_uint256; }
    function _w_ptr_to_uint256() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_to_bytes20() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_bytes20() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_bytes20(address pv) internal view returns (uint256) { return uint256(uint160(bytes20(address(pv)))); }
    uint256 _res_modifier_to_bytes20;
    modifier _m_modifier_to_bytes20(address pv) { _res_modifier_to_bytes20 = uint256(uint160(bytes20(address(pv)))); _; }
    function _e_modifier_to_bytes20(address rd) internal _m_modifier_to_bytes20(rd) returns (uint256) { return _res_modifier_to_bytes20; }
    function _w_ptr_to_bytes20() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_to_address_payable() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_address_payable() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_address_payable(address pv) internal view returns (uint256) { return uint256(uint160(address(payable(address(pv))))); }
    function t_local_to_uint160() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint160() external view returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint160()))); }
    function t_ret_to_uint160() external view returns (uint256) { address rx = _s_ret_to_uint160(); return uint256(uint160(address(rx))); }
    function t_param_to_uint160() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint160(rx); }
    function t_copy_to_uint160() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(address(ry))); }
    function t_ternary_to_uint160() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(address(ry))); }
    function t_ifjoin_to_uint160() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(address(ry))); }
    function t_loopjoin_to_uint160() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(address(ry))); }
    function t_tuple_to_uint160() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(address(ry))); }
    function t_modifier_to_uint160() external returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_uint160(rx); }
    function t_ptr_to_uint160() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint160; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(address(ry))); }
    function t_mem_to_uint160() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_uint256() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint256() external view returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint256()))); }
    function t_ret_to_uint256() external view returns (uint256) { address rx = _s_ret_to_uint256(); return uint256(uint160(address(rx))); }
    function t_param_to_uint256() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint256(rx); }
    function t_copy_to_uint256() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(address(ry))); }
    function t_ternary_to_uint256() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(address(ry))); }
    function t_ifjoin_to_uint256() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(address(ry))); }
    function t_loopjoin_to_uint256() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(address(ry))); }
    function t_tuple_to_uint256() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(address(ry))); }
    function t_modifier_to_uint256() external returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_uint256(rx); }
    function t_ptr_to_uint256() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint256; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(address(ry))); }
    function t_mem_to_uint256() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_bytes20() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(bytes20(address(rx)))); }
    function t_retdirect_to_bytes20() external view returns (uint256) { return uint256(uint160(bytes20(address(_s_retdirect_to_bytes20())))); }
    function t_ret_to_bytes20() external view returns (uint256) { address rx = _s_ret_to_bytes20(); return uint256(uint160(bytes20(address(rx)))); }
    function t_param_to_bytes20() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_bytes20(rx); }
    function t_copy_to_bytes20() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(bytes20(address(ry)))); }
    function t_ternary_to_bytes20() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(bytes20(address(ry)))); }
    function t_ifjoin_to_bytes20() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(bytes20(address(ry)))); }
    function t_loopjoin_to_bytes20() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(bytes20(address(ry)))); }
    function t_tuple_to_bytes20() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(bytes20(address(ry)))); }
    function t_modifier_to_bytes20() external returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_bytes20(rx); }
    function t_ptr_to_bytes20() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes20; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(bytes20(address(ry)))); }
    function t_mem_to_bytes20() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(bytes20(address(ry)))); }
    function t_local_to_address_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(payable(address(rx))))); }
    function t_retdirect_to_address_payable() external view returns (uint256) { return uint256(uint160(address(payable(address(_s_retdirect_to_address_payable()))))); }
    function t_ret_to_address_payable() external view returns (uint256) { address rx = _s_ret_to_address_payable(); return uint256(uint160(address(payable(address(rx))))); }
    function t_param_to_address_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_address_payable(rx); }
}
contract Caddr_1 {
    uint256 _res_modifier_to_address_payable;
    modifier _m_modifier_to_address_payable(address pv) { _res_modifier_to_address_payable = uint256(uint160(address(payable(address(pv))))); _; }
    function _e_modifier_to_address_payable(address rd) internal _m_modifier_to_address_payable(rd) returns (uint256) { return _res_modifier_to_address_payable; }
    function _w_ptr_to_address_payable() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_to_uint64() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_uint64() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint64(address pv) internal view returns (uint256) { return uint256(uint64(uint160(address(pv)))); }
    uint256 _res_modifier_to_uint64;
    modifier _m_modifier_to_uint64(address pv) { _res_modifier_to_uint64 = uint256(uint64(uint160(address(pv)))); _; }
    function _e_modifier_to_uint64(address rd) internal _m_modifier_to_uint64(rd) returns (uint256) { return _res_modifier_to_uint64; }
    function _w_ptr_to_uint64() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_to_uint8() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_uint8() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint8(address pv) internal view returns (uint256) { return uint256(uint8(uint160(address(pv)))); }
    uint256 _res_modifier_to_uint8;
    modifier _m_modifier_to_uint8(address pv) { _res_modifier_to_uint8 = uint256(uint8(uint160(address(pv)))); _; }
    function _e_modifier_to_uint8(address rd) internal _m_modifier_to_uint8(rd) returns (uint256) { return _res_modifier_to_uint8; }
    function _w_ptr_to_uint8() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_to_payable() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_payable() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_payable(address pv) internal view returns (uint256) { return uint256(uint160(address(payable(pv)))); }
    function t_copy_to_address_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(address(payable(address(ry))))); }
    function t_ternary_to_address_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(address(payable(address(ry))))); }
    function t_ifjoin_to_address_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(address(payable(address(ry))))); }
    function t_loopjoin_to_address_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(address(payable(address(ry))))); }
    function t_tuple_to_address_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(address(payable(address(ry))))); }
    function t_modifier_to_address_payable() external returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_address_payable(rx); }
    function t_ptr_to_address_payable() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_address_payable; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(address(payable(address(ry))))); }
    function t_mem_to_address_payable() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(address(payable(address(ry))))); }
    function t_local_to_uint64() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint64(uint160(address(rx)))); }
    function t_retdirect_to_uint64() external view returns (uint256) { return uint256(uint64(uint160(address(_s_retdirect_to_uint64())))); }
    function t_ret_to_uint64() external view returns (uint256) { address rx = _s_ret_to_uint64(); return uint256(uint64(uint160(address(rx)))); }
    function t_param_to_uint64() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint64(rx); }
    function t_copy_to_uint64() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint64(uint160(address(ry)))); }
    function t_ternary_to_uint64() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint64(uint160(address(ry)))); }
    function t_ifjoin_to_uint64() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint64(uint160(address(ry)))); }
    function t_loopjoin_to_uint64() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint64(uint160(address(ry)))); }
    function t_tuple_to_uint64() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint64(uint160(address(ry)))); }
    function t_modifier_to_uint64() external returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_uint64(rx); }
    function t_ptr_to_uint64() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint64; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint64(uint160(address(ry)))); }
    function t_mem_to_uint64() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint64(uint160(address(ry)))); }
    function t_local_to_uint8() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint8(uint160(address(rx)))); }
    function t_retdirect_to_uint8() external view returns (uint256) { return uint256(uint8(uint160(address(_s_retdirect_to_uint8())))); }
    function t_ret_to_uint8() external view returns (uint256) { address rx = _s_ret_to_uint8(); return uint256(uint8(uint160(address(rx)))); }
    function t_param_to_uint8() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint8(rx); }
    function t_copy_to_uint8() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint8(uint160(address(ry)))); }
    function t_ternary_to_uint8() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint8(uint160(address(ry)))); }
    function t_ifjoin_to_uint8() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(uint160(address(ry)))); }
    function t_loopjoin_to_uint8() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(uint160(address(ry)))); }
    function t_tuple_to_uint8() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(uint160(address(ry)))); }
    function t_modifier_to_uint8() external returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_uint8(rx); }
    function t_ptr_to_uint8() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint8; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint8(uint160(address(ry)))); }
    function t_mem_to_uint8() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint8(uint160(address(ry)))); }
    function t_local_to_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(payable(rx)))); }
    function t_retdirect_to_payable() external view returns (uint256) { return uint256(uint160(address(payable(_s_retdirect_to_payable())))); }
    function t_ret_to_payable() external view returns (uint256) { address rx = _s_ret_to_payable(); return uint256(uint160(address(payable(rx)))); }
    function t_param_to_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_payable(rx); }
    function t_copy_to_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(address(payable(ry)))); }
    function t_ternary_to_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(address(payable(ry)))); }
    function t_ifjoin_to_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(address(payable(ry)))); }
    function t_loopjoin_to_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(address(payable(ry)))); }
}
contract Caddr_2 {
    uint256 _res_modifier_to_payable;
    modifier _m_modifier_to_payable(address pv) { _res_modifier_to_payable = uint256(uint160(address(payable(pv)))); _; }
    function _e_modifier_to_payable(address rd) internal _m_modifier_to_payable(rd) returns (uint256) { return _res_modifier_to_payable; }
    function _w_ptr_to_payable() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_tuple_to_payable() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(address(payable(ry)))); }
    function t_modifier_to_payable() external returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_payable(rx); }
    function t_ptr_to_payable() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_payable; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(address(payable(ry)))); }
    function t_mem_to_payable() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(address(payable(ry)))); }
}
contract Caddrp_0 {
    function _s_retdirect_to_uint160() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint160(address payable pv) internal view returns (uint256) { return uint256(uint160(address(pv))); }
    function _s_retdirect_to_uint256() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint256(address payable pv) internal view returns (uint256) { return uint256(uint160(address(pv))); }
    function _s_retdirect_to_bytes20() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_bytes20(address payable pv) internal view returns (uint256) { return uint256(uint160(bytes20(address(pv)))); }
    function _s_retdirect_to_address_payable() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_address_payable(address payable pv) internal view returns (uint256) { return uint256(uint160(address(payable(address(pv))))); }
    function _s_retdirect_to_uint64() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint64(address payable pv) internal view returns (uint256) { return uint256(uint64(uint160(address(pv)))); }
    function _s_retdirect_to_uint8() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint8(address payable pv) internal view returns (uint256) { return uint256(uint8(uint160(address(pv)))); }
    function t_local_to_uint160() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint160() external view returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint160()))); }
    function t_param_to_uint160() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint160(rx); }
    function t_mem_to_uint160() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_uint256() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint256() external view returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint256()))); }
    function t_param_to_uint256() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint256(rx); }
    function t_mem_to_uint256() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_bytes20() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(bytes20(address(rx)))); }
    function t_retdirect_to_bytes20() external view returns (uint256) { return uint256(uint160(bytes20(address(_s_retdirect_to_bytes20())))); }
    function t_param_to_bytes20() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_bytes20(rx); }
    function t_mem_to_bytes20() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint160(bytes20(address(ry)))); }
    function t_local_to_address_payable() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(payable(address(rx))))); }
    function t_retdirect_to_address_payable() external view returns (uint256) { return uint256(uint160(address(payable(address(_s_retdirect_to_address_payable()))))); }
    function t_param_to_address_payable() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_address_payable(rx); }
    function t_mem_to_address_payable() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint160(address(payable(address(ry))))); }
    function t_local_to_uint64() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint64(uint160(address(rx)))); }
    function t_retdirect_to_uint64() external view returns (uint256) { return uint256(uint64(uint160(address(_s_retdirect_to_uint64())))); }
    function t_param_to_uint64() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint64(rx); }
    function t_mem_to_uint64() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint64(uint160(address(ry)))); }
    function t_local_to_uint8() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint8(uint160(address(rx)))); }
    function t_retdirect_to_uint8() external view returns (uint256) { return uint256(uint8(uint160(address(_s_retdirect_to_uint8())))); }
    function t_param_to_uint8() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint8(rx); }
    function t_mem_to_uint8() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint8(uint160(address(ry)))); }
}
contract Cctr_0 {
    function _s_retdirect_to_uint160() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint160(IC pv) internal view returns (uint256) { return uint256(uint160(address(pv))); }
    function _s_retdirect_to_uint256() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint256(IC pv) internal view returns (uint256) { return uint256(uint160(address(pv))); }
    function _s_retdirect_to_bytes20() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_bytes20(IC pv) internal view returns (uint256) { return uint256(uint160(bytes20(address(pv)))); }
    function _s_retdirect_to_address_payable() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_address_payable(IC pv) internal view returns (uint256) { return uint256(uint160(address(payable(address(pv))))); }
    function _s_retdirect_to_uint64() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint64(IC pv) internal view returns (uint256) { return uint256(uint64(uint160(address(pv)))); }
    function _s_retdirect_to_uint8() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint8(IC pv) internal view returns (uint256) { return uint256(uint8(uint160(address(pv)))); }
    function _s_retdirect_to_ic_addr() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_ic_addr(IC pv) internal view returns (uint256) { return uint256(uint160(address(IC(address(pv))))); }
    function t_local_to_uint160() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint160() external view returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint160()))); }
    function t_param_to_uint160() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint160(rx); }
    function t_mem_to_uint160() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_uint256() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint256() external view returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint256()))); }
    function t_param_to_uint256() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint256(rx); }
    function t_mem_to_uint256() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_bytes20() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(bytes20(address(rx)))); }
    function t_retdirect_to_bytes20() external view returns (uint256) { return uint256(uint160(bytes20(address(_s_retdirect_to_bytes20())))); }
    function t_param_to_bytes20() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_bytes20(rx); }
    function t_mem_to_bytes20() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(bytes20(address(ry)))); }
    function t_local_to_address_payable() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(payable(address(rx))))); }
    function t_retdirect_to_address_payable() external view returns (uint256) { return uint256(uint160(address(payable(address(_s_retdirect_to_address_payable()))))); }
    function t_param_to_address_payable() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_address_payable(rx); }
    function t_mem_to_address_payable() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(address(payable(address(ry))))); }
    function t_local_to_uint64() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint64(uint160(address(rx)))); }
    function t_retdirect_to_uint64() external view returns (uint256) { return uint256(uint64(uint160(address(_s_retdirect_to_uint64())))); }
    function t_param_to_uint64() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint64(rx); }
    function t_mem_to_uint64() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint64(uint160(address(ry)))); }
    function t_local_to_uint8() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint8(uint160(address(rx)))); }
    function t_retdirect_to_uint8() external view returns (uint256) { return uint256(uint8(uint160(address(_s_retdirect_to_uint8())))); }
    function t_param_to_uint8() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint8(rx); }
    function t_mem_to_uint8() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint8(uint160(address(ry)))); }
    function t_local_to_ic_addr() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(IC(address(rx))))); }
    function t_retdirect_to_ic_addr() external view returns (uint256) { return uint256(uint160(address(IC(address(_s_retdirect_to_ic_addr()))))); }
    function t_param_to_ic_addr() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_ic_addr(rx); }
    function t_mem_to_ic_addr() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(address(IC(address(ry))))); }
}
contract Cbool_0 {
    function _s_retdirect_to_uint_via_ternary() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_to_uint_via_ternary(bool pv) internal view returns (uint256) { return pv ? 1 : 0; }
    function _s_retdirect_to_uint_via_bytes() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_to_uint_via_bytes(bool pv) internal view returns (uint256) { return uint256(uint8(pv ? 1 : 0)); }
    function t_local_to_uint_via_ternary() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return rx ? 1 : 0; }
    function t_retdirect_to_uint_via_ternary() external view returns (uint256) { return _s_retdirect_to_uint_via_ternary() ? 1 : 0; }
    function t_param_to_uint_via_ternary() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_to_uint_via_ternary(rx); }
    function t_mem_to_uint_via_ternary() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return ry ? 1 : 0; }
    function t_local_to_uint_via_bytes() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(uint8(rx ? 1 : 0)); }
    function t_retdirect_to_uint_via_bytes() external view returns (uint256) { return uint256(uint8(_s_retdirect_to_uint_via_bytes() ? 1 : 0)); }
    function t_param_to_uint_via_bytes() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_to_uint_via_bytes(rx); }
    function t_mem_to_uint_via_bytes() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(uint8(ry ? 1 : 0)); }
}
contract Cb1_0 {
    function _s_retdirect_to_bytes2() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes2(bytes1 pv) internal view returns (uint256) { return uint256(uint16(bytes2(pv))); }
    function _s_retdirect_to_bytes4() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes4(bytes1 pv) internal view returns (uint256) { return uint256(uint32(bytes4(pv))); }
    function _s_retdirect_to_bytes8() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes8(bytes1 pv) internal view returns (uint256) { return uint256(uint64(bytes8(pv))); }
    function _s_retdirect_to_bytes16() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes16(bytes1 pv) internal view returns (uint256) { return uint256(uint128(bytes16(pv))); }
    function _s_retdirect_to_bytes20() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes20(bytes1 pv) internal view returns (uint256) { return uint256(uint160(bytes20(pv))); }
    function _s_retdirect_to_bytes32() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes32(bytes1 pv) internal view returns (uint256) { return uint256(uint256(bytes32(pv))); }
    function _s_retdirect_to_uint() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_uint(bytes1 pv) internal view returns (uint256) { return uint256(uint8(pv)); }
    function _s_retdirect_to_uint_wide() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_uint_wide(bytes1 pv) internal view returns (uint256) { return uint256(uint8(pv)) >> 0; }
    function _s_retdirect_to_bytes_shift_back() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes_shift_back(bytes1 pv) internal view returns (uint256) { return uint256(uint8(bytes1(uint8(pv)))); }
    function t_local_to_bytes2() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint16(bytes2(rx))); }
    function t_retdirect_to_bytes2() external view returns (uint256) { return uint256(uint16(bytes2(_s_retdirect_to_bytes2()))); }
    function t_param_to_bytes2() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes2(rx); }
    function t_mem_to_bytes2() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint16(bytes2(ry))); }
    function t_local_to_bytes4() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint32(bytes4(rx))); }
    function t_retdirect_to_bytes4() external view returns (uint256) { return uint256(uint32(bytes4(_s_retdirect_to_bytes4()))); }
    function t_param_to_bytes4() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes4(rx); }
    function t_mem_to_bytes4() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint32(bytes4(ry))); }
    function t_local_to_bytes8() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint64(bytes8(rx))); }
    function t_retdirect_to_bytes8() external view returns (uint256) { return uint256(uint64(bytes8(_s_retdirect_to_bytes8()))); }
    function t_param_to_bytes8() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes8(rx); }
    function t_mem_to_bytes8() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint64(bytes8(ry))); }
    function t_local_to_bytes16() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint128(bytes16(rx))); }
    function t_retdirect_to_bytes16() external view returns (uint256) { return uint256(uint128(bytes16(_s_retdirect_to_bytes16()))); }
    function t_param_to_bytes16() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes16(rx); }
    function t_mem_to_bytes16() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint128(bytes16(ry))); }
    function t_local_to_bytes20() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint160(bytes20(rx))); }
    function t_retdirect_to_bytes20() external view returns (uint256) { return uint256(uint160(bytes20(_s_retdirect_to_bytes20()))); }
    function t_param_to_bytes20() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes20(rx); }
    function t_mem_to_bytes20() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint160(bytes20(ry))); }
    function t_local_to_bytes32() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint256(bytes32(rx))); }
    function t_retdirect_to_bytes32() external view returns (uint256) { return uint256(uint256(bytes32(_s_retdirect_to_bytes32()))); }
    function t_param_to_bytes32() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes32(rx); }
    function t_mem_to_bytes32() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint256(bytes32(ry))); }
    function t_local_to_uint() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx)); }
    function t_retdirect_to_uint() external view returns (uint256) { return uint256(uint8(_s_retdirect_to_uint())); }
    function t_param_to_uint() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_uint(rx); }
    function t_mem_to_uint() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_to_uint_wide() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx)) >> 0; }
    function t_retdirect_to_uint_wide() external view returns (uint256) { return uint256(uint8(_s_retdirect_to_uint_wide())) >> 0; }
    function t_param_to_uint_wide() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_uint_wide(rx); }
    function t_mem_to_uint_wide() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry)) >> 0; }
    function t_local_to_bytes_shift_back() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(bytes1(uint8(rx)))); }
    function t_retdirect_to_bytes_shift_back() external view returns (uint256) { return uint256(uint8(bytes1(uint8(_s_retdirect_to_bytes_shift_back())))); }
    function t_param_to_bytes_shift_back() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes_shift_back(rx); }
    function t_mem_to_bytes_shift_back() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(bytes1(uint8(ry)))); }
}
contract Cb4_0 {
    function _s_retdirect_to_bytes1() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes1() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes1(bytes4 pv) internal view returns (uint256) { return uint256(uint8(bytes1(pv))); }
    uint256 _res_modifier_to_bytes1;
    modifier _m_modifier_to_bytes1(bytes4 pv) { _res_modifier_to_bytes1 = uint256(uint8(bytes1(pv))); _; }
    function _e_modifier_to_bytes1(bytes4 rd) internal _m_modifier_to_bytes1(rd) returns (uint256) { return _res_modifier_to_bytes1; }
    function _w_ptr_to_bytes1() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes2() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes2() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes2(bytes4 pv) internal view returns (uint256) { return uint256(uint16(bytes2(pv))); }
    uint256 _res_modifier_to_bytes2;
    modifier _m_modifier_to_bytes2(bytes4 pv) { _res_modifier_to_bytes2 = uint256(uint16(bytes2(pv))); _; }
    function _e_modifier_to_bytes2(bytes4 rd) internal _m_modifier_to_bytes2(rd) returns (uint256) { return _res_modifier_to_bytes2; }
    function _w_ptr_to_bytes2() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes8() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes8() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes8(bytes4 pv) internal view returns (uint256) { return uint256(uint64(bytes8(pv))); }
    uint256 _res_modifier_to_bytes8;
    modifier _m_modifier_to_bytes8(bytes4 pv) { _res_modifier_to_bytes8 = uint256(uint64(bytes8(pv))); _; }
    function _e_modifier_to_bytes8(bytes4 rd) internal _m_modifier_to_bytes8(rd) returns (uint256) { return _res_modifier_to_bytes8; }
    function _w_ptr_to_bytes8() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes16() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes16() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes16(bytes4 pv) internal view returns (uint256) { return uint256(uint128(bytes16(pv))); }
    function t_local_to_bytes1() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint8(bytes1(rx))); }
    function t_retdirect_to_bytes1() external view returns (uint256) { return uint256(uint8(bytes1(_s_retdirect_to_bytes1()))); }
    function t_ret_to_bytes1() external view returns (uint256) { bytes4 rx = _s_ret_to_bytes1(); return uint256(uint8(bytes1(rx))); }
    function t_param_to_bytes1() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes1(rx); }
    function t_copy_to_bytes1() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint8(bytes1(ry))); }
    function t_ternary_to_bytes1() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint8(bytes1(ry))); }
    function t_ifjoin_to_bytes1() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(bytes1(ry))); }
    function t_loopjoin_to_bytes1() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(bytes1(ry))); }
    function t_tuple_to_bytes1() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(bytes1(ry))); }
    function t_modifier_to_bytes1() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes1(rx); }
    function t_ptr_to_bytes1() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes1; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint8(bytes1(ry))); }
    function t_mem_to_bytes1() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint8(bytes1(ry))); }
    function t_local_to_bytes2() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint16(bytes2(rx))); }
    function t_retdirect_to_bytes2() external view returns (uint256) { return uint256(uint16(bytes2(_s_retdirect_to_bytes2()))); }
    function t_ret_to_bytes2() external view returns (uint256) { bytes4 rx = _s_ret_to_bytes2(); return uint256(uint16(bytes2(rx))); }
    function t_param_to_bytes2() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes2(rx); }
    function t_copy_to_bytes2() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint16(bytes2(ry))); }
    function t_ternary_to_bytes2() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint16(bytes2(ry))); }
    function t_ifjoin_to_bytes2() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint16(bytes2(ry))); }
    function t_loopjoin_to_bytes2() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint16(bytes2(ry))); }
    function t_tuple_to_bytes2() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint16(bytes2(ry))); }
    function t_modifier_to_bytes2() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes2(rx); }
    function t_ptr_to_bytes2() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes2; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint16(bytes2(ry))); }
    function t_mem_to_bytes2() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint16(bytes2(ry))); }
    function t_local_to_bytes8() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint64(bytes8(rx))); }
    function t_retdirect_to_bytes8() external view returns (uint256) { return uint256(uint64(bytes8(_s_retdirect_to_bytes8()))); }
    function t_ret_to_bytes8() external view returns (uint256) { bytes4 rx = _s_ret_to_bytes8(); return uint256(uint64(bytes8(rx))); }
    function t_param_to_bytes8() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes8(rx); }
    function t_copy_to_bytes8() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint64(bytes8(ry))); }
    function t_ternary_to_bytes8() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint64(bytes8(ry))); }
    function t_ifjoin_to_bytes8() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint64(bytes8(ry))); }
    function t_loopjoin_to_bytes8() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint64(bytes8(ry))); }
    function t_tuple_to_bytes8() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint64(bytes8(ry))); }
    function t_modifier_to_bytes8() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes8(rx); }
    function t_ptr_to_bytes8() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes8; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint64(bytes8(ry))); }
    function t_mem_to_bytes8() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint64(bytes8(ry))); }
    function t_local_to_bytes16() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint128(bytes16(rx))); }
    function t_retdirect_to_bytes16() external view returns (uint256) { return uint256(uint128(bytes16(_s_retdirect_to_bytes16()))); }
    function t_ret_to_bytes16() external view returns (uint256) { bytes4 rx = _s_ret_to_bytes16(); return uint256(uint128(bytes16(rx))); }
    function t_param_to_bytes16() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes16(rx); }
}
contract Cb4_1 {
    uint256 _res_modifier_to_bytes16;
    modifier _m_modifier_to_bytes16(bytes4 pv) { _res_modifier_to_bytes16 = uint256(uint128(bytes16(pv))); _; }
    function _e_modifier_to_bytes16(bytes4 rd) internal _m_modifier_to_bytes16(rd) returns (uint256) { return _res_modifier_to_bytes16; }
    function _w_ptr_to_bytes16() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes20() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes20() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes20(bytes4 pv) internal view returns (uint256) { return uint256(uint160(bytes20(pv))); }
    uint256 _res_modifier_to_bytes20;
    modifier _m_modifier_to_bytes20(bytes4 pv) { _res_modifier_to_bytes20 = uint256(uint160(bytes20(pv))); _; }
    function _e_modifier_to_bytes20(bytes4 rd) internal _m_modifier_to_bytes20(rd) returns (uint256) { return _res_modifier_to_bytes20; }
    function _w_ptr_to_bytes20() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes32() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes32() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes32(bytes4 pv) internal view returns (uint256) { return uint256(uint256(bytes32(pv))); }
    uint256 _res_modifier_to_bytes32;
    modifier _m_modifier_to_bytes32(bytes4 pv) { _res_modifier_to_bytes32 = uint256(uint256(bytes32(pv))); _; }
    function _e_modifier_to_bytes32(bytes4 rd) internal _m_modifier_to_bytes32(rd) returns (uint256) { return _res_modifier_to_bytes32; }
    function _w_ptr_to_bytes32() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_uint() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_uint() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_uint(bytes4 pv) internal view returns (uint256) { return uint256(uint32(pv)); }
    function t_copy_to_bytes16() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint128(bytes16(ry))); }
    function t_ternary_to_bytes16() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint128(bytes16(ry))); }
    function t_ifjoin_to_bytes16() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint128(bytes16(ry))); }
    function t_loopjoin_to_bytes16() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint128(bytes16(ry))); }
    function t_tuple_to_bytes16() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint128(bytes16(ry))); }
    function t_modifier_to_bytes16() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes16(rx); }
    function t_ptr_to_bytes16() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes16; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint128(bytes16(ry))); }
    function t_mem_to_bytes16() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint128(bytes16(ry))); }
    function t_local_to_bytes20() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint160(bytes20(rx))); }
    function t_retdirect_to_bytes20() external view returns (uint256) { return uint256(uint160(bytes20(_s_retdirect_to_bytes20()))); }
    function t_ret_to_bytes20() external view returns (uint256) { bytes4 rx = _s_ret_to_bytes20(); return uint256(uint160(bytes20(rx))); }
    function t_param_to_bytes20() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes20(rx); }
    function t_copy_to_bytes20() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint160(bytes20(ry))); }
    function t_ternary_to_bytes20() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint160(bytes20(ry))); }
    function t_ifjoin_to_bytes20() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(bytes20(ry))); }
    function t_loopjoin_to_bytes20() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(bytes20(ry))); }
    function t_tuple_to_bytes20() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(bytes20(ry))); }
    function t_modifier_to_bytes20() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes20(rx); }
    function t_ptr_to_bytes20() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes20; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint160(bytes20(ry))); }
    function t_mem_to_bytes20() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint160(bytes20(ry))); }
    function t_local_to_bytes32() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint256(bytes32(rx))); }
    function t_retdirect_to_bytes32() external view returns (uint256) { return uint256(uint256(bytes32(_s_retdirect_to_bytes32()))); }
    function t_ret_to_bytes32() external view returns (uint256) { bytes4 rx = _s_ret_to_bytes32(); return uint256(uint256(bytes32(rx))); }
    function t_param_to_bytes32() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes32(rx); }
    function t_copy_to_bytes32() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint256(bytes32(ry))); }
    function t_ternary_to_bytes32() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint256(bytes32(ry))); }
    function t_ifjoin_to_bytes32() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint256(bytes32(ry))); }
    function t_loopjoin_to_bytes32() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint256(bytes32(ry))); }
    function t_tuple_to_bytes32() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint256(bytes32(ry))); }
    function t_modifier_to_bytes32() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes32(rx); }
    function t_ptr_to_bytes32() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes32; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint256(bytes32(ry))); }
    function t_mem_to_bytes32() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint256(bytes32(ry))); }
    function t_local_to_uint() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(rx)); }
    function t_retdirect_to_uint() external view returns (uint256) { return uint256(uint32(_s_retdirect_to_uint())); }
    function t_ret_to_uint() external view returns (uint256) { bytes4 rx = _s_ret_to_uint(); return uint256(uint32(rx)); }
    function t_param_to_uint() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_uint(rx); }
    function t_copy_to_uint() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(ry)); }
    function t_ternary_to_uint() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(ry)); }
    function t_ifjoin_to_uint() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry)); }
    function t_loopjoin_to_uint() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry)); }
}
contract Cb4_2 {
    uint256 _res_modifier_to_uint;
    modifier _m_modifier_to_uint(bytes4 pv) { _res_modifier_to_uint = uint256(uint32(pv)); _; }
    function _e_modifier_to_uint(bytes4 rd) internal _m_modifier_to_uint(rd) returns (uint256) { return _res_modifier_to_uint; }
    function _w_ptr_to_uint() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_uint_wide() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_uint_wide() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_uint_wide(bytes4 pv) internal view returns (uint256) { return uint256(uint32(pv)) >> 0; }
    uint256 _res_modifier_to_uint_wide;
    modifier _m_modifier_to_uint_wide(bytes4 pv) { _res_modifier_to_uint_wide = uint256(uint32(pv)) >> 0; _; }
    function _e_modifier_to_uint_wide(bytes4 rd) internal _m_modifier_to_uint_wide(rd) returns (uint256) { return _res_modifier_to_uint_wide; }
    function _w_ptr_to_uint_wide() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes_shift_back() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes_shift_back() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes_shift_back(bytes4 pv) internal view returns (uint256) { return uint256(uint32(bytes4(uint32(pv)))); }
    uint256 _res_modifier_to_bytes_shift_back;
    modifier _m_modifier_to_bytes_shift_back(bytes4 pv) { _res_modifier_to_bytes_shift_back = uint256(uint32(bytes4(uint32(pv)))); _; }
    function _e_modifier_to_bytes_shift_back(bytes4 rd) internal _m_modifier_to_bytes_shift_back(rd) returns (uint256) { return _res_modifier_to_bytes_shift_back; }
    function _w_ptr_to_bytes_shift_back() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_tuple_to_uint() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry)); }
    function t_modifier_to_uint() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_uint(rx); }
    function t_ptr_to_uint() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(ry)); }
    function t_mem_to_uint() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(ry)); }
    function t_local_to_uint_wide() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(rx)) >> 0; }
    function t_retdirect_to_uint_wide() external view returns (uint256) { return uint256(uint32(_s_retdirect_to_uint_wide())) >> 0; }
    function t_ret_to_uint_wide() external view returns (uint256) { bytes4 rx = _s_ret_to_uint_wide(); return uint256(uint32(rx)) >> 0; }
    function t_param_to_uint_wide() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_uint_wide(rx); }
    function t_copy_to_uint_wide() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(ry)) >> 0; }
    function t_ternary_to_uint_wide() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(ry)) >> 0; }
    function t_ifjoin_to_uint_wide() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry)) >> 0; }
    function t_loopjoin_to_uint_wide() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry)) >> 0; }
    function t_tuple_to_uint_wide() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry)) >> 0; }
    function t_modifier_to_uint_wide() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_uint_wide(rx); }
    function t_ptr_to_uint_wide() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint_wide; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(ry)) >> 0; }
    function t_mem_to_uint_wide() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(ry)) >> 0; }
    function t_local_to_bytes_shift_back() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(bytes4(uint32(rx)))); }
    function t_retdirect_to_bytes_shift_back() external view returns (uint256) { return uint256(uint32(bytes4(uint32(_s_retdirect_to_bytes_shift_back())))); }
    function t_ret_to_bytes_shift_back() external view returns (uint256) { bytes4 rx = _s_ret_to_bytes_shift_back(); return uint256(uint32(bytes4(uint32(rx)))); }
    function t_param_to_bytes_shift_back() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes_shift_back(rx); }
    function t_copy_to_bytes_shift_back() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(bytes4(uint32(ry)))); }
    function t_ternary_to_bytes_shift_back() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(bytes4(uint32(ry)))); }
    function t_ifjoin_to_bytes_shift_back() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(bytes4(uint32(ry)))); }
    function t_loopjoin_to_bytes_shift_back() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(bytes4(uint32(ry)))); }
    function t_tuple_to_bytes_shift_back() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(bytes4(uint32(ry)))); }
    function t_modifier_to_bytes_shift_back() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes_shift_back(rx); }
    function t_ptr_to_bytes_shift_back() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes_shift_back; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(bytes4(uint32(ry)))); }
    function t_mem_to_bytes_shift_back() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(bytes4(uint32(ry)))); }
}
contract Cb20_0 {
    function _s_retdirect_to_bytes1() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes1(bytes20 pv) internal view returns (uint256) { return uint256(uint8(bytes1(pv))); }
    function _s_retdirect_to_bytes2() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes2(bytes20 pv) internal view returns (uint256) { return uint256(uint16(bytes2(pv))); }
    function _s_retdirect_to_bytes4() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes4(bytes20 pv) internal view returns (uint256) { return uint256(uint32(bytes4(pv))); }
    function _s_retdirect_to_bytes8() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes8(bytes20 pv) internal view returns (uint256) { return uint256(uint64(bytes8(pv))); }
    function _s_retdirect_to_bytes16() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes16(bytes20 pv) internal view returns (uint256) { return uint256(uint128(bytes16(pv))); }
    function _s_retdirect_to_bytes32() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes32(bytes20 pv) internal view returns (uint256) { return uint256(uint256(bytes32(pv))); }
    function _s_retdirect_to_uint() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_uint(bytes20 pv) internal view returns (uint256) { return uint256(uint160(pv)); }
    function _s_retdirect_to_uint_wide() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_uint_wide(bytes20 pv) internal view returns (uint256) { return uint256(uint160(pv)) >> 0; }
    function _s_retdirect_to_bytes_shift_back() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes_shift_back(bytes20 pv) internal view returns (uint256) { return uint256(uint160(bytes20(uint160(pv)))); }
    function _s_retdirect_to_address() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_address(bytes20 pv) internal view returns (uint256) { return uint256(uint160(address(pv))); }
    function t_local_to_bytes1() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint8(bytes1(rx))); }
    function t_retdirect_to_bytes1() external view returns (uint256) { return uint256(uint8(bytes1(_s_retdirect_to_bytes1()))); }
    function t_param_to_bytes1() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes1(rx); }
    function t_mem_to_bytes1() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint8(bytes1(ry))); }
    function t_local_to_bytes2() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint16(bytes2(rx))); }
    function t_retdirect_to_bytes2() external view returns (uint256) { return uint256(uint16(bytes2(_s_retdirect_to_bytes2()))); }
    function t_param_to_bytes2() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes2(rx); }
    function t_mem_to_bytes2() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint16(bytes2(ry))); }
    function t_local_to_bytes4() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint32(bytes4(rx))); }
    function t_retdirect_to_bytes4() external view returns (uint256) { return uint256(uint32(bytes4(_s_retdirect_to_bytes4()))); }
    function t_param_to_bytes4() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes4(rx); }
    function t_mem_to_bytes4() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint32(bytes4(ry))); }
    function t_local_to_bytes8() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint64(bytes8(rx))); }
    function t_retdirect_to_bytes8() external view returns (uint256) { return uint256(uint64(bytes8(_s_retdirect_to_bytes8()))); }
    function t_param_to_bytes8() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes8(rx); }
    function t_mem_to_bytes8() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint64(bytes8(ry))); }
    function t_local_to_bytes16() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint128(bytes16(rx))); }
    function t_retdirect_to_bytes16() external view returns (uint256) { return uint256(uint128(bytes16(_s_retdirect_to_bytes16()))); }
    function t_param_to_bytes16() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes16(rx); }
    function t_mem_to_bytes16() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint128(bytes16(ry))); }
    function t_local_to_bytes32() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint256(bytes32(rx))); }
    function t_retdirect_to_bytes32() external view returns (uint256) { return uint256(uint256(bytes32(_s_retdirect_to_bytes32()))); }
    function t_param_to_bytes32() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes32(rx); }
    function t_mem_to_bytes32() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint256(bytes32(ry))); }
    function t_local_to_uint() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(rx)); }
    function t_retdirect_to_uint() external view returns (uint256) { return uint256(uint160(_s_retdirect_to_uint())); }
    function t_param_to_uint() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_uint(rx); }
    function t_mem_to_uint() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(ry)); }
    function t_local_to_uint_wide() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(rx)) >> 0; }
    function t_retdirect_to_uint_wide() external view returns (uint256) { return uint256(uint160(_s_retdirect_to_uint_wide())) >> 0; }
    function t_param_to_uint_wide() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_uint_wide(rx); }
    function t_mem_to_uint_wide() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(ry)) >> 0; }
    function t_local_to_bytes_shift_back() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(bytes20(uint160(rx)))); }
    function t_retdirect_to_bytes_shift_back() external view returns (uint256) { return uint256(uint160(bytes20(uint160(_s_retdirect_to_bytes_shift_back())))); }
    function t_param_to_bytes_shift_back() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes_shift_back(rx); }
    function t_mem_to_bytes_shift_back() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(bytes20(uint160(ry)))); }
    function t_local_to_address() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(address(rx))); }
    function t_retdirect_to_address() external view returns (uint256) { return uint256(uint160(address(_s_retdirect_to_address()))); }
    function t_param_to_address() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_address(rx); }
    function t_mem_to_address() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(address(ry))); }
}
contract Cen_0 {
    function _s_retdirect_to_uint8() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_to_uint8() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_to_uint8(E pv) internal view returns (uint256) { return uint256(uint8(pv)); }
    uint256 _res_modifier_to_uint8;
    modifier _m_modifier_to_uint8(E pv) { _res_modifier_to_uint8 = uint256(uint8(pv)); _; }
    function _e_modifier_to_uint8(E rd) internal _m_modifier_to_uint8(rd) returns (uint256) { return _res_modifier_to_uint8; }
    function _w_ptr_to_uint8() internal pure returns (uint256) { return 0x101; }
    function _s_retdirect_to_uint256() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_to_uint256() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_to_uint256(E pv) internal view returns (uint256) { return uint256(uint256(uint8(pv))); }
    uint256 _res_modifier_to_uint256;
    modifier _m_modifier_to_uint256(E pv) { _res_modifier_to_uint256 = uint256(uint256(uint8(pv))); _; }
    function _e_modifier_to_uint256(E rd) internal _m_modifier_to_uint256(rd) returns (uint256) { return _res_modifier_to_uint256; }
    function _w_ptr_to_uint256() internal pure returns (uint256) { return 0x101; }
    function _s_retdirect_to_uint16() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_to_uint16() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_to_uint16(E pv) internal view returns (uint256) { return uint256(uint16(uint8(pv))); }
    uint256 _res_modifier_to_uint16;
    modifier _m_modifier_to_uint16(E pv) { _res_modifier_to_uint16 = uint256(uint16(uint8(pv))); _; }
    function _e_modifier_to_uint16(E rd) internal _m_modifier_to_uint16(rd) returns (uint256) { return _res_modifier_to_uint16; }
    function _w_ptr_to_uint16() internal pure returns (uint256) { return 0x101; }
    function _s_retdirect_roundtrip() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_roundtrip() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_roundtrip(E pv) internal view returns (uint256) { return uint256(uint8(E(uint8(uint8(pv))))); }
    function t_local_to_uint8() external view returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(uint8(rx)); }
    function t_retdirect_to_uint8() external view returns (uint256) { return uint256(uint8(_s_retdirect_to_uint8())); }
    function t_ret_to_uint8() external view returns (uint256) { E rx = _s_ret_to_uint8(); return uint256(uint8(rx)); }
    function t_param_to_uint8() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_to_uint8(rx); }
    function t_copy_to_uint8() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(uint8(ry)); }
    function t_ternary_to_uint8() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(uint8(ry)); }
    function t_ifjoin_to_uint8() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(ry)); }
    function t_loopjoin_to_uint8() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(ry)); }
    function t_tuple_to_uint8() external view returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(ry)); }
    function t_modifier_to_uint8() external returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_to_uint8(rx); }
    function t_ptr_to_uint8() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint8; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(uint8(ry)); }
    function t_mem_to_uint8() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_to_uint256() external view returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(uint256(uint8(rx))); }
    function t_retdirect_to_uint256() external view returns (uint256) { return uint256(uint256(uint8(_s_retdirect_to_uint256()))); }
    function t_ret_to_uint256() external view returns (uint256) { E rx = _s_ret_to_uint256(); return uint256(uint256(uint8(rx))); }
    function t_param_to_uint256() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_to_uint256(rx); }
    function t_copy_to_uint256() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(uint256(uint8(ry))); }
    function t_ternary_to_uint256() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(uint256(uint8(ry))); }
    function t_ifjoin_to_uint256() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint256(uint8(ry))); }
    function t_loopjoin_to_uint256() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint256(uint8(ry))); }
    function t_tuple_to_uint256() external view returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint256(uint8(ry))); }
    function t_modifier_to_uint256() external returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_to_uint256(rx); }
    function t_ptr_to_uint256() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint256; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(uint256(uint8(ry))); }
    function t_mem_to_uint256() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(uint256(uint8(ry))); }
    function t_local_to_uint16() external view returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(uint16(uint8(rx))); }
    function t_retdirect_to_uint16() external view returns (uint256) { return uint256(uint16(uint8(_s_retdirect_to_uint16()))); }
    function t_ret_to_uint16() external view returns (uint256) { E rx = _s_ret_to_uint16(); return uint256(uint16(uint8(rx))); }
    function t_param_to_uint16() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_to_uint16(rx); }
    function t_copy_to_uint16() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(uint16(uint8(ry))); }
    function t_ternary_to_uint16() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(uint16(uint8(ry))); }
    function t_ifjoin_to_uint16() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint16(uint8(ry))); }
    function t_loopjoin_to_uint16() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint16(uint8(ry))); }
    function t_tuple_to_uint16() external view returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint16(uint8(ry))); }
    function t_modifier_to_uint16() external returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_to_uint16(rx); }
    function t_ptr_to_uint16() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint16; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(uint16(uint8(ry))); }
    function t_mem_to_uint16() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(uint16(uint8(ry))); }
    function t_local_roundtrip() external view returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(uint8(E(uint8(uint8(rx))))); }
    function t_retdirect_roundtrip() external view returns (uint256) { return uint256(uint8(E(uint8(uint8(_s_retdirect_roundtrip()))))); }
    function t_ret_roundtrip() external view returns (uint256) { E rx = _s_ret_roundtrip(); return uint256(uint8(E(uint8(uint8(rx))))); }
    function t_param_roundtrip() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_roundtrip(rx); }
}
contract Cen_1 {
    uint256 _res_modifier_roundtrip;
    modifier _m_modifier_roundtrip(E pv) { _res_modifier_roundtrip = uint256(uint8(E(uint8(uint8(pv))))); _; }
    function _e_modifier_roundtrip(E rd) internal _m_modifier_roundtrip(rd) returns (uint256) { return _res_modifier_roundtrip; }
    function _w_ptr_roundtrip() internal pure returns (uint256) { return 0x101; }
    function t_copy_roundtrip() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_ternary_roundtrip() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_ifjoin_roundtrip() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_loopjoin_roundtrip() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_tuple_roundtrip() external view returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_modifier_roundtrip() external returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_roundtrip(rx); }
    function t_ptr_roundtrip() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_roundtrip; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_mem_roundtrip() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(uint8(E(uint8(uint8(ry))))); }
}
contract Cud_0 {
    function _s_retdirect_unwrap_wide() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_unwrap_wide(U pv) internal view returns (uint256) { return uint256(uint16(U.unwrap(pv))); }
    function _s_retdirect_unwrap_u256() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_unwrap_u256(U pv) internal view returns (uint256) { return uint256(U.unwrap(pv)); }
    function _s_retdirect_rewrap() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_rewrap(U pv) internal view returns (uint256) { return uint256(U.unwrap(U.wrap(U.unwrap(pv)))); }
    function _s_retdirect_unwrap_to_bytes() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_unwrap_to_bytes(U pv) internal view returns (uint256) { return uint256(uint8(bytes1(U.unwrap(pv)))); }
    function t_local_unwrap_wide() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(uint16(U.unwrap(rx))); }
    function t_retdirect_unwrap_wide() external view returns (uint256) { return uint256(uint16(U.unwrap(_s_retdirect_unwrap_wide()))); }
    function t_param_unwrap_wide() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_unwrap_wide(rx); }
    function t_mem_unwrap_wide() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(uint16(U.unwrap(ry))); }
    function t_local_unwrap_u256() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(U.unwrap(rx)); }
    function t_retdirect_unwrap_u256() external view returns (uint256) { return uint256(U.unwrap(_s_retdirect_unwrap_u256())); }
    function t_param_unwrap_u256() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_unwrap_u256(rx); }
    function t_mem_unwrap_u256() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(U.unwrap(ry)); }
    function t_local_rewrap() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(U.unwrap(U.wrap(U.unwrap(rx)))); }
    function t_retdirect_rewrap() external view returns (uint256) { return uint256(U.unwrap(U.wrap(U.unwrap(_s_retdirect_rewrap())))); }
    function t_param_rewrap() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_rewrap(rx); }
    function t_mem_rewrap() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(U.unwrap(U.wrap(U.unwrap(ry)))); }
    function t_local_unwrap_to_bytes() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(uint8(bytes1(U.unwrap(rx)))); }
    function t_retdirect_unwrap_to_bytes() external view returns (uint256) { return uint256(uint8(bytes1(U.unwrap(_s_retdirect_unwrap_to_bytes())))); }
    function t_param_unwrap_to_bytes() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_unwrap_to_bytes(rx); }
    function t_mem_unwrap_to_bytes() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(uint8(bytes1(U.unwrap(ry)))); }
}
