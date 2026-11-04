//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_ext_arg => 27
//@ run-call: Cu8_0::t_retdirect_ext_arg => 27
//@ run-call: Cu8_0::t_param_ext_arg => 27
//@ run-call: Cu8_0::t_mem_ext_arg => 27
//@ run-call: Cu8_0::t_local_ext_arg_second => 27
//@ run-call: Cu8_0::t_retdirect_ext_arg_second => 27
//@ run-call: Cu8_0::t_param_ext_arg_second => 27
//@ run-call: Cu8_0::t_mem_ext_arg_second => 27
//@ run-call: Cu8_0::t_local_ext_try => 27
//@ run-call: Cu8_0::t_retdirect_ext_try => 27
//@ run-call: Cu8_0::t_param_ext_try => 27
//@ run-call: Cu8_0::t_mem_ext_try => 27
//@ run-call: Cu8_0::t_local_ext_try_catch_arg => 0
//@ run-call: Cu8_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Cu8_0::t_param_ext_try_catch_arg => 0
//@ run-call: Cu8_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Cu8_0::t_local_ext_static => 27
//@ run-call: Cu8_0::t_retdirect_ext_static => 27
//@ run-call: Cu8_0::t_param_ext_static => 27
//@ run-call: Cu8_0::t_mem_ext_static => 27
//@ run-call: Cu8_0::t_local_ext_lowlevel_encode => 27
//@ run-call: Cu8_0::t_retdirect_ext_lowlevel_encode => 27
//@ run-call: Cu8_0::t_param_ext_lowlevel_encode => 27
//@ run-call: Cu8_0::t_mem_ext_lowlevel_encode => 27
//@ run-call: Cu8_0::t_local_ext_new_arg => 27
//@ run-call: Cu8_0::t_retdirect_ext_new_arg => 27
//@ run-call: Cu8_0::t_param_ext_new_arg => 27
//@ run-call: Cu8_0::t_mem_ext_new_arg => 27
//@ run-call: Cu8_0::t_local_ctor_arg => 27
//@ run-call: Cu8_0::t_retdirect_ctor_arg => 27
//@ run-call: Cu8_0::t_param_ctor_arg => 27
//@ run-call: Cu8_0::t_mem_ctor_arg => 27
//@ run-call: Cu8_0::t_local_opt_value; value=100 => 27
//@ run-call: Cu8_0::t_retdirect_opt_value; value=100 => 27
//@ run-call: Cu8_0::t_param_opt_value; value=100 => 27
//@ run-call: Cu8_0::t_mem_opt_value; value=100 => 27
//@ run-call: Cu8_0::t_local_opt_gas => 1
//@ run-call: Cu8_0::t_retdirect_opt_gas => 1
//@ run-call: Cu8_0::t_param_opt_gas => 1
//@ run-call: Cu8_0::t_mem_opt_gas => 1
//@ run-call: Cu8_1::t_local_opt_salt => 1
//@ run-call: Cu8_1::t_retdirect_opt_salt => 1
//@ run-call: Cu8_1::t_param_opt_salt => 1
//@ run-call: Cu8_1::t_mem_opt_salt => 1
//@ run-call: Cu64_0::t_local_ext_arg => 27
//@ run-call: Cu64_0::t_retdirect_ext_arg => 27
//@ run-call: Cu64_0::t_param_ext_arg => 27
//@ run-call: Cu64_0::t_mem_ext_arg => 27
//@ run-call: Cu64_0::t_local_ext_arg_second => 27
//@ run-call: Cu64_0::t_retdirect_ext_arg_second => 27
//@ run-call: Cu64_0::t_param_ext_arg_second => 27
//@ run-call: Cu64_0::t_mem_ext_arg_second => 27
//@ run-call: Cu64_0::t_local_ext_try => 27
//@ run-call: Cu64_0::t_retdirect_ext_try => 27
//@ run-call: Cu64_0::t_param_ext_try => 27
//@ run-call: Cu64_0::t_mem_ext_try => 27
//@ run-call: Cu64_0::t_local_ext_try_catch_arg => 0
//@ run-call: Cu64_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Cu64_0::t_param_ext_try_catch_arg => 0
//@ run-call: Cu64_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Cu64_0::t_local_ext_static => 27
//@ run-call: Cu64_0::t_retdirect_ext_static => 27
//@ run-call: Cu64_0::t_param_ext_static => 27
//@ run-call: Cu64_0::t_mem_ext_static => 27
//@ run-call: Cu64_0::t_local_ext_lowlevel_encode => 27
//@ run-call: Cu64_0::t_retdirect_ext_lowlevel_encode => 27
//@ run-call: Cu64_0::t_param_ext_lowlevel_encode => 27
//@ run-call: Cu64_0::t_mem_ext_lowlevel_encode => 27
//@ run-call: Cu64_0::t_local_ext_new_arg => 27
//@ run-call: Cu64_0::t_retdirect_ext_new_arg => 27
//@ run-call: Cu64_0::t_param_ext_new_arg => 27
//@ run-call: Cu64_0::t_mem_ext_new_arg => 27
//@ run-call: Cu64_0::t_local_ctor_arg => 27
//@ run-call: Cu64_0::t_retdirect_ctor_arg => 27
//@ run-call: Cu64_0::t_param_ctor_arg => 27
//@ run-call: Cu64_0::t_mem_ctor_arg => 27
//@ run-call: Cu64_0::t_local_opt_value; value=100 => 27
//@ run-call: Cu64_0::t_retdirect_opt_value; value=100 => 27
//@ run-call: Cu64_0::t_param_opt_value; value=100 => 27
//@ run-call: Cu64_0::t_mem_opt_value; value=100 => 27
//@ run-call: Cu64_0::t_local_opt_gas => 1
//@ run-call: Cu64_0::t_retdirect_opt_gas => 1
//@ run-call: Cu64_0::t_param_opt_gas => 1
//@ run-call: Cu64_0::t_mem_opt_gas => 1
//@ run-call: Cu64_1::t_local_opt_salt => 1
//@ run-call: Cu64_1::t_retdirect_opt_salt => 1
//@ run-call: Cu64_1::t_param_opt_salt => 1
//@ run-call: Cu64_1::t_mem_opt_salt => 1
//@ run-call: Ci8_0::t_local_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ext_try_catch_arg => 0
//@ run-call: Ci8_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Ci8_0::t_param_ext_try_catch_arg => 0
//@ run-call: Ci8_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Ci8_0::t_local_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_arg_second => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_try => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_try_catch_arg => 0
//@ run-call: Ci24_0::t_retdirect_ext_try_catch_arg => 0
//@ run-call: Ci24_0::t_param_ext_try_catch_arg => 0
//@ run-call: Ci24_0::t_mem_ext_try_catch_arg => 0
//@ run-call: Ci24_0::t_local_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_static => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_lowlevel_encode => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ext_new_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
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
//@ run-call: Caddr_0::t_local_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_addr_balance; value=7 => 7
//@ run-call: Caddr_0::t_retdirect_addr_balance; value=7 => 7
//@ run-call: Caddr_0::t_param_addr_balance; value=7 => 7
//@ run-call: Caddr_0::t_mem_addr_balance; value=7 => 7
//@ run-call: Caddr_0::t_local_addr_code_len => 1
//@ run-call: Caddr_0::t_retdirect_addr_code_len => 1
//@ run-call: Caddr_0::t_param_addr_code_len => 1
//@ run-call: Caddr_0::t_mem_addr_code_len => 1
//@ run-call: Caddr_1::t_local_addr_codehash => 1
//@ run-call: Caddr_1::t_retdirect_addr_codehash => 1
//@ run-call: Caddr_1::t_param_addr_codehash => 1
//@ run-call: Caddr_1::t_mem_addr_codehash => 1
//@ run-call: Caddr_1::t_local_addr_call => 77
//@ run-call: Caddr_1::t_retdirect_addr_call => 77
//@ run-call: Caddr_1::t_param_addr_call => 77
//@ run-call: Caddr_1::t_mem_addr_call => 77
//@ run-call: Caddr_1::t_local_addr_staticcall => 77
//@ run-call: Caddr_1::t_retdirect_addr_staticcall => 77
//@ run-call: Caddr_1::t_param_addr_staticcall => 77
//@ run-call: Caddr_1::t_mem_addr_staticcall => 77
//@ run-call: Caddr_1::t_local_addr_delegatecall => 77
//@ run-call: Caddr_1::t_retdirect_addr_delegatecall => 77
//@ run-call: Caddr_1::t_param_addr_delegatecall => 77
//@ run-call: Caddr_1::t_mem_addr_delegatecall => 77
//@ run-call: Caddr_1::t_local_addr_ext_call => 1
//@ run-call: Caddr_1::t_retdirect_addr_ext_call => 1
//@ run-call: Caddr_1::t_param_addr_ext_call => 1
//@ run-call: Caddr_1::t_mem_addr_ext_call => 1
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
//@ run-call: Caddrp_0::t_local_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_addr_balance; value=7 => 7
//@ run-call: Caddrp_0::t_retdirect_addr_balance; value=7 => 7
//@ run-call: Caddrp_0::t_param_addr_balance; value=7 => 7
//@ run-call: Caddrp_0::t_mem_addr_balance; value=7 => 7
//@ run-call: Caddrp_0::t_local_addr_code_len => 1
//@ run-call: Caddrp_0::t_retdirect_addr_code_len => 1
//@ run-call: Caddrp_0::t_param_addr_code_len => 1
//@ run-call: Caddrp_0::t_mem_addr_code_len => 1
//@ run-call: Caddrp_1::t_local_addr_codehash => 1
//@ run-call: Caddrp_1::t_retdirect_addr_codehash => 1
//@ run-call: Caddrp_1::t_param_addr_codehash => 1
//@ run-call: Caddrp_1::t_mem_addr_codehash => 1
//@ run-call: Caddrp_1::t_local_addr_call => 77
//@ run-call: Caddrp_1::t_retdirect_addr_call => 77
//@ run-call: Caddrp_1::t_param_addr_call => 77
//@ run-call: Caddrp_1::t_mem_addr_call => 77
//@ run-call: Caddrp_1::t_local_addr_staticcall => 77
//@ run-call: Caddrp_1::t_retdirect_addr_staticcall => 77
//@ run-call: Caddrp_1::t_param_addr_staticcall => 77
//@ run-call: Caddrp_1::t_mem_addr_staticcall => 77
//@ run-call: Caddrp_1::t_local_addr_delegatecall => 77
//@ run-call: Caddrp_1::t_retdirect_addr_delegatecall => 77
//@ run-call: Caddrp_1::t_param_addr_delegatecall => 77
//@ run-call: Caddrp_1::t_mem_addr_delegatecall => 77
//@ run-call: Caddrp_1::t_local_addr_ext_call => 1
//@ run-call: Caddrp_1::t_retdirect_addr_ext_call => 1
//@ run-call: Caddrp_1::t_param_addr_ext_call => 1
//@ run-call: Caddrp_1::t_mem_addr_ext_call => 1
//@ run-call: Caddrp_1::t_local_addr_send; value=7 => 1
//@ run-call: Caddrp_1::t_retdirect_addr_send; value=7 => 1
//@ run-call: Caddrp_1::t_param_addr_send; value=7 => 1
//@ run-call: Caddrp_1::t_mem_addr_send; value=7 => 1
//@ run-call: Caddrp_1::t_local_addr_transfer; value=7 => 1
//@ run-call: Caddrp_1::t_retdirect_addr_transfer; value=7 => 1
//@ run-call: Caddrp_1::t_param_addr_transfer; value=7 => 1
//@ run-call: Caddrp_1::t_mem_addr_transfer; value=7 => 1
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
//@ run-call: Cctr_0::t_local_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_addr_balance; value=7 => 7
//@ run-call: Cctr_0::t_retdirect_addr_balance; value=7 => 7
//@ run-call: Cctr_0::t_param_addr_balance; value=7 => 7
//@ run-call: Cctr_0::t_mem_addr_balance; value=7 => 7
//@ run-call: Cctr_0::t_local_addr_code_len => 1
//@ run-call: Cctr_0::t_retdirect_addr_code_len => 1
//@ run-call: Cctr_0::t_param_addr_code_len => 1
//@ run-call: Cctr_0::t_mem_addr_code_len => 1
//@ run-call: Cctr_1::t_local_addr_codehash => 1
//@ run-call: Cctr_1::t_retdirect_addr_codehash => 1
//@ run-call: Cctr_1::t_param_addr_codehash => 1
//@ run-call: Cctr_1::t_mem_addr_codehash => 1
//@ run-call: Cctr_1::t_local_addr_call => 77
//@ run-call: Cctr_1::t_retdirect_addr_call => 77
//@ run-call: Cctr_1::t_param_addr_call => 77
//@ run-call: Cctr_1::t_mem_addr_call => 77
//@ run-call: Cctr_1::t_local_addr_staticcall => 77
//@ run-call: Cctr_1::t_retdirect_addr_staticcall => 77
//@ run-call: Cctr_1::t_param_addr_staticcall => 77
//@ run-call: Cctr_1::t_mem_addr_staticcall => 77
//@ run-call: Cctr_1::t_local_addr_delegatecall => 77
//@ run-call: Cctr_1::t_retdirect_addr_delegatecall => 77
//@ run-call: Cctr_1::t_param_addr_delegatecall => 77
//@ run-call: Cctr_1::t_mem_addr_delegatecall => 77
//@ run-call: Cctr_1::t_local_addr_ext_call => 1
//@ run-call: Cctr_1::t_retdirect_addr_ext_call => 1
//@ run-call: Cctr_1::t_param_addr_ext_call => 1
//@ run-call: Cctr_1::t_mem_addr_ext_call => 1
//@ run-call: Cctr_1::t_local_ctr_ext_call => 1
//@ run-call: Cctr_1::t_retdirect_ctr_ext_call => 1
//@ run-call: Cctr_1::t_param_ctr_ext_call => 1
//@ run-call: Cctr_1::t_mem_ctr_ext_call => 1
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
//@ run-call: Cbool_0::t_local_ctor_arg => 1
//@ run-call: Cbool_0::t_retdirect_ctor_arg => 1
//@ run-call: Cbool_0::t_param_ctor_arg => 1
//@ run-call: Cbool_0::t_mem_ctor_arg => 1
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
//@ run-call: Cb1_0::t_local_ctor_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_ctor_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_ctor_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_ctor_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
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
//@ run-call: Cb4_0::t_local_ctor_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_ctor_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_ctor_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_ctor_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
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
//@ run-call: Cb20_0::t_local_ctor_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_ctor_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_ctor_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_ctor_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
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
//@ run-call-fail: Cen_0::t_local_ctor_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_ctor_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_ctor_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_ctor_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
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
//@ run-call: Cud_0::t_local_ctor_arg => 27
//@ run-call: Cud_0::t_retdirect_ctor_arg => 27
//@ run-call: Cud_0::t_param_ctor_arg => 27
//@ run-call: Cud_0::t_mem_ctor_arg => 27

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: external calls, try/catch, call options and constructors.

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
    function _s_retdirect_ext_arg() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_arg(uint8 pv) internal  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_arg_second(uint8 pv) internal  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f2(uint8(3), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_try(uint8 pv) internal  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_try_catch_arg(uint8 pv) internal  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_static(uint8 pv) internal  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_lowlevel_encode(uint8 pv) internal  returns (uint256) { Callee_u8 c = new Callee_u8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u8.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_new_arg(uint8 pv) internal  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ctor_arg(uint8 pv) internal  returns (uint256) { Ctor_u8 k = new Ctor_u8(pv); uint256 r = k.got(); return r; }
    function _s_retdirect_opt_value() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_opt_value(uint8 pv) internal  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.g{value: pv}(); return r; }
    function _s_retdirect_opt_gas() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_opt_gas(uint8 pv) internal  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.h{gas: 100000 + pv}(); return r; }
    function t_local_ext_arg() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.f2(uint8(3), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f2(uint8(3), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.f2(uint8(3), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u8.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_u8 c = new Callee_u8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u8.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u8.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Ctor_u8 k = new Ctor_u8(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_u8 k = new Ctor_u8(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Ctor_u8 k = new Ctor_u8(ry); uint256 r = k.got(); return r; }
    function t_local_opt_value() external payable returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.g{value: rx}(); return r; }
    function t_retdirect_opt_value() external payable returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.g{value: _s_retdirect_opt_value()}(); return r; }
    function t_param_opt_value() external payable returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_opt_value(rx); }
    function t_mem_opt_value() external payable returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.g{value: ry}(); return r; }
    function t_local_opt_gas() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.h{gas: 100000 + rx}(); return r; }
    function t_retdirect_opt_gas() external  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.h{gas: 100000 + _s_retdirect_opt_gas()}(); return r; }
    function t_param_opt_gas() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_opt_gas(rx); }
    function t_mem_opt_gas() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.h{gas: 100000 + ry}(); return r; }
}
contract Cu8_1 {
    function _s_retdirect_opt_salt() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_opt_salt(uint8 pv) internal  returns (uint256) { Ctor_u8 k = new Ctor_u8{salt: bytes32(uint256(pv))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(pv)), keccak256(abi.encodePacked(type(Ctor_u8).creationCode, abi.encode(uint8(1))))))))); return address(k) == p ? 1 : 0; }
    function t_local_opt_salt() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Ctor_u8 k = new Ctor_u8{salt: bytes32(uint256(rx))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(rx)), keccak256(abi.encodePacked(type(Ctor_u8).creationCode, abi.encode(uint8(1))))))))); return address(k) == p ? 1 : 0; }
    function t_retdirect_opt_salt() external  returns (uint256) { Ctor_u8 k = new Ctor_u8{salt: bytes32(uint256(_s_retdirect_opt_salt()))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(_s_retdirect_opt_salt())), keccak256(abi.encodePacked(type(Ctor_u8).creationCode, abi.encode(uint8(1))))))))); return address(k) == p ? 1 : 0; }
    function t_param_opt_salt() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_opt_salt(rx); }
    function t_mem_opt_salt() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Ctor_u8 k = new Ctor_u8{salt: bytes32(uint256(ry))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(ry)), keccak256(abi.encodePacked(type(Ctor_u8).creationCode, abi.encode(uint8(1))))))))); return address(k) == p ? 1 : 0; }
}
contract Cu64_0 {
    function _s_retdirect_ext_arg() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_arg(uint64 pv) internal  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_arg_second(uint64 pv) internal  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f2(uint64(3), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_try(uint64 pv) internal  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_try_catch_arg(uint64 pv) internal  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_static(uint64 pv) internal  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_lowlevel_encode(uint64 pv) internal  returns (uint256) { Callee_u64 c = new Callee_u64(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u64.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ext_new_arg(uint64 pv) internal  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ctor_arg(uint64 pv) internal  returns (uint256) { Ctor_u64 k = new Ctor_u64(pv); uint256 r = k.got(); return r; }
    function _s_retdirect_opt_value() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_opt_value(uint64 pv) internal  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.g{value: pv}(); return r; }
    function _s_retdirect_opt_gas() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_opt_gas(uint64 pv) internal  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.h{gas: 100000 + pv}(); return r; }
    function t_local_ext_arg() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.f2(uint64(3), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f2(uint64(3), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.f2(uint64(3), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u64.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_u64 c = new Callee_u64(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u64.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_u64.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Ctor_u64 k = new Ctor_u64(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_u64 k = new Ctor_u64(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Ctor_u64 k = new Ctor_u64(ry); uint256 r = k.got(); return r; }
    function t_local_opt_value() external payable returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.g{value: rx}(); return r; }
    function t_retdirect_opt_value() external payable returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.g{value: _s_retdirect_opt_value()}(); return r; }
    function t_param_opt_value() external payable returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_opt_value(rx); }
    function t_mem_opt_value() external payable returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.g{value: ry}(); return r; }
    function t_local_opt_gas() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.h{gas: 100000 + rx}(); return r; }
    function t_retdirect_opt_gas() external  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.h{gas: 100000 + _s_retdirect_opt_gas()}(); return r; }
    function t_param_opt_gas() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_opt_gas(rx); }
    function t_mem_opt_gas() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.h{gas: 100000 + ry}(); return r; }
}
contract Cu64_1 {
    function _s_retdirect_opt_salt() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_opt_salt(uint64 pv) internal  returns (uint256) { Ctor_u64 k = new Ctor_u64{salt: bytes32(uint256(pv))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(pv)), keccak256(abi.encodePacked(type(Ctor_u64).creationCode, abi.encode(uint64(1))))))))); return address(k) == p ? 1 : 0; }
    function t_local_opt_salt() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Ctor_u64 k = new Ctor_u64{salt: bytes32(uint256(rx))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(rx)), keccak256(abi.encodePacked(type(Ctor_u64).creationCode, abi.encode(uint64(1))))))))); return address(k) == p ? 1 : 0; }
    function t_retdirect_opt_salt() external  returns (uint256) { Ctor_u64 k = new Ctor_u64{salt: bytes32(uint256(_s_retdirect_opt_salt()))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(_s_retdirect_opt_salt())), keccak256(abi.encodePacked(type(Ctor_u64).creationCode, abi.encode(uint64(1))))))))); return address(k) == p ? 1 : 0; }
    function t_param_opt_salt() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_opt_salt(rx); }
    function t_mem_opt_salt() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Ctor_u64 k = new Ctor_u64{salt: bytes32(uint256(ry))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(ry)), keccak256(abi.encodePacked(type(Ctor_u64).creationCode, abi.encode(uint64(1))))))))); return address(k) == p ? 1 : 0; }
}
contract Ci8_0 {
    function _s_retdirect_ext_arg() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_arg(int8 pv) internal  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_arg_second(int8 pv) internal  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f2(int8(-2), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_try(int8 pv) internal  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_try_catch_arg(int8 pv) internal  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_static(int8 pv) internal  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_lowlevel_encode(int8 pv) internal  returns (uint256) { Callee_i8 c = new Callee_i8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i8.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ext_new_arg(int8 pv) internal  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ctor_arg(int8 pv) internal  returns (uint256) { Ctor_i8 k = new Ctor_i8(pv); uint256 r = k.got(); return r; }
    function t_local_ext_arg() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r = c.f2(int8(-2), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f2(int8(-2), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r = c.f2(int8(-2), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i8.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_i8 c = new Callee_i8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i8.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i8.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Callee_i8 c = new Callee_i8(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_i8 c = new Callee_i8(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Callee_i8 c = new Callee_i8(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Ctor_i8 k = new Ctor_i8(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_i8 k = new Ctor_i8(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Ctor_i8 k = new Ctor_i8(ry); uint256 r = k.got(); return r; }
}
contract Ci24_0 {
    function _s_retdirect_ext_arg() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_arg(int24 pv) internal  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_arg_second(int24 pv) internal  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f2(int24(-2), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_try(int24 pv) internal  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_try_catch_arg(int24 pv) internal  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_static(int24 pv) internal  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_lowlevel_encode(int24 pv) internal  returns (uint256) { Callee_i24 c = new Callee_i24(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i24.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ext_new_arg(int24 pv) internal  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ctor_arg(int24 pv) internal  returns (uint256) { Ctor_i24 k = new Ctor_i24(pv); uint256 r = k.got(); return r; }
    function t_local_ext_arg() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r = c.f2(int24(-2), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f2(int24(-2), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r = c.f2(int24(-2), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i24.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_i24 c = new Callee_i24(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i24.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_i24.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Callee_i24 c = new Callee_i24(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_i24 c = new Callee_i24(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Callee_i24 c = new Callee_i24(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Ctor_i24 k = new Ctor_i24(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_i24 k = new Ctor_i24(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Ctor_i24 k = new Ctor_i24(ry); uint256 r = k.got(); return r; }
}
contract Caddr_0 {
    function ping() external pure returns (uint256) { return 77; }
    receive() external payable {}
    function f() external {}
    function _s_retdirect_ext_arg() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg(address pv) internal  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg_second(address pv) internal  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f2(address(0x1111111111111111111111111111111111111111), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try(address pv) internal  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try_catch_arg(address pv) internal  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_static(address pv) internal  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_lowlevel_encode(address pv) internal  returns (uint256) { Callee_addr c = new Callee_addr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addr.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_new_arg(address pv) internal  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ctor_arg(address pv) internal  returns (uint256) { Ctor_addr k = new Ctor_addr(pv); uint256 r = k.got(); return r; }
    function _s_retdirect_addr_balance() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_balance(address pv) internal  returns (uint256) { return address(pv).balance; }
    function _s_retdirect_addr_code_len() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_code_len(address pv) internal view returns (uint256) { return address(pv).code.length == address(this).code.length ? 1 : 0; }
    function t_local_ext_arg() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r = c.f2(address(0x1111111111111111111111111111111111111111), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f2(address(0x1111111111111111111111111111111111111111), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r = c.f2(address(0x1111111111111111111111111111111111111111), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addr.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_addr c = new Callee_addr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addr.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addr.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addr c = new Callee_addr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_addr c = new Callee_addr(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Callee_addr c = new Callee_addr(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Ctor_addr k = new Ctor_addr(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_addr k = new Ctor_addr(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Ctor_addr k = new Ctor_addr(ry); uint256 r = k.got(); return r; }
    function t_local_addr_balance() external payable returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).balance; }
    function t_retdirect_addr_balance() external payable returns (uint256) { return address(_s_retdirect_addr_balance()).balance; }
    function t_param_addr_balance() external payable returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_balance(rx); }
    function t_mem_addr_balance() external payable returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; return address(ry).balance; }
    function t_local_addr_code_len() external view returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).code.length == address(this).code.length ? 1 : 0; }
    function t_retdirect_addr_code_len() external view returns (uint256) { return address(_s_retdirect_addr_code_len()).code.length == address(this).code.length ? 1 : 0; }
    function t_param_addr_code_len() external view returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_code_len(rx); }
    function t_mem_addr_code_len() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; return address(ry).code.length == address(this).code.length ? 1 : 0; }
}
contract Caddr_1 {
    function ping() external pure returns (uint256) { return 77; }
    receive() external payable {}
    function f() external {}
    function _s_retdirect_addr_codehash() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_codehash(address pv) internal view returns (uint256) { return address(pv).codehash == address(this).codehash ? 1 : 0; }
    function _s_retdirect_addr_call() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_call(address pv) internal  returns (uint256) { (bool ok, bytes memory out) = address(pv).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_staticcall() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_staticcall(address pv) internal view returns (uint256) { (bool ok, bytes memory out) = address(pv).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_delegatecall() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_delegatecall(address pv) internal  returns (uint256) { (bool ok, bytes memory out) = address(pv).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_ext_call() internal view returns (address r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_ext_call(address pv) internal  returns (uint256) { IC(address(pv)).f(); return 1; }
    function t_local_addr_codehash() external view returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).codehash == address(this).codehash ? 1 : 0; }
    function t_retdirect_addr_codehash() external view returns (uint256) { return address(_s_retdirect_addr_codehash()).codehash == address(this).codehash ? 1 : 0; }
    function t_param_addr_codehash() external view returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_codehash(rx); }
    function t_mem_addr_codehash() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; return address(ry).codehash == address(this).codehash ? 1 : 0; }
    function t_local_addr_call() external  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_call() external  returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_call()).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_call() external  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_call(rx); }
    function t_mem_addr_call() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; (bool ok, bytes memory out) = address(ry).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_staticcall() external view returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_staticcall() external view returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_staticcall()).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_staticcall() external view returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_staticcall(rx); }
    function t_mem_addr_staticcall() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; (bool ok, bytes memory out) = address(ry).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_delegatecall() external  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_delegatecall() external  returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_delegatecall()).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_delegatecall() external  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_delegatecall(rx); }
    function t_mem_addr_delegatecall() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; (bool ok, bytes memory out) = address(ry).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_ext_call() external  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } IC(address(rx)).f(); return 1; }
    function t_retdirect_addr_ext_call() external  returns (uint256) { IC(address(_s_retdirect_addr_ext_call())).f(); return 1; }
    function t_param_addr_ext_call() external  returns (uint256) { address rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_ext_call(rx); }
    function t_mem_addr_ext_call() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address ry = ra[0]; IC(address(ry)).f(); return 1; }
}
contract Caddrp_0 {
    function ping() external pure returns (uint256) { return 77; }
    receive() external payable {}
    function f() external {}
    function _s_retdirect_ext_arg() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg(address payable pv) internal  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg_second(address payable pv) internal  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f2(payable(address(0x1111111111111111111111111111111111111111)), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try(address payable pv) internal  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try_catch_arg(address payable pv) internal  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_static(address payable pv) internal  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_lowlevel_encode(address payable pv) internal  returns (uint256) { Callee_addrp c = new Callee_addrp(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addrp.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_new_arg(address payable pv) internal  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ctor_arg(address payable pv) internal  returns (uint256) { Ctor_addrp k = new Ctor_addrp(pv); uint256 r = k.got(); return r; }
    function _s_retdirect_addr_balance() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_balance(address payable pv) internal  returns (uint256) { return address(pv).balance; }
    function _s_retdirect_addr_code_len() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_code_len(address payable pv) internal view returns (uint256) { return address(pv).code.length == address(this).code.length ? 1 : 0; }
    function t_local_ext_arg() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r = c.f2(payable(address(0x1111111111111111111111111111111111111111)), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f2(payable(address(0x1111111111111111111111111111111111111111)), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r = c.f2(payable(address(0x1111111111111111111111111111111111111111)), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addrp.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_addrp c = new Callee_addrp(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addrp.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_addrp.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_addrp c = new Callee_addrp(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_addrp c = new Callee_addrp(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Callee_addrp c = new Callee_addrp(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Ctor_addrp k = new Ctor_addrp(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_addrp k = new Ctor_addrp(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Ctor_addrp k = new Ctor_addrp(ry); uint256 r = k.got(); return r; }
    function t_local_addr_balance() external payable returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).balance; }
    function t_retdirect_addr_balance() external payable returns (uint256) { return address(_s_retdirect_addr_balance()).balance; }
    function t_param_addr_balance() external payable returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_balance(rx); }
    function t_mem_addr_balance() external payable returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; return address(ry).balance; }
    function t_local_addr_code_len() external view returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).code.length == address(this).code.length ? 1 : 0; }
    function t_retdirect_addr_code_len() external view returns (uint256) { return address(_s_retdirect_addr_code_len()).code.length == address(this).code.length ? 1 : 0; }
    function t_param_addr_code_len() external view returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_code_len(rx); }
    function t_mem_addr_code_len() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; return address(ry).code.length == address(this).code.length ? 1 : 0; }
}
contract Caddrp_1 {
    function ping() external pure returns (uint256) { return 77; }
    receive() external payable {}
    function f() external {}
    function _s_retdirect_addr_codehash() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_codehash(address payable pv) internal view returns (uint256) { return address(pv).codehash == address(this).codehash ? 1 : 0; }
    function _s_retdirect_addr_call() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_call(address payable pv) internal  returns (uint256) { (bool ok, bytes memory out) = address(pv).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_staticcall() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_staticcall(address payable pv) internal view returns (uint256) { (bool ok, bytes memory out) = address(pv).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_delegatecall() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_delegatecall(address payable pv) internal  returns (uint256) { (bool ok, bytes memory out) = address(pv).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_ext_call() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_ext_call(address payable pv) internal  returns (uint256) { IC(address(pv)).f(); return 1; }
    function _s_retdirect_addr_send() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_send(address payable pv) internal  returns (uint256) { bool ok = pv.send(1); return ok ? 1 : 0; }
    function _s_retdirect_addr_transfer() internal view returns (address payable r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_transfer(address payable pv) internal  returns (uint256) { pv.transfer(1); return 1; }
    function t_local_addr_codehash() external view returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).codehash == address(this).codehash ? 1 : 0; }
    function t_retdirect_addr_codehash() external view returns (uint256) { return address(_s_retdirect_addr_codehash()).codehash == address(this).codehash ? 1 : 0; }
    function t_param_addr_codehash() external view returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_codehash(rx); }
    function t_mem_addr_codehash() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; return address(ry).codehash == address(this).codehash ? 1 : 0; }
    function t_local_addr_call() external  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_call() external  returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_call()).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_call() external  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_call(rx); }
    function t_mem_addr_call() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; (bool ok, bytes memory out) = address(ry).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_staticcall() external view returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_staticcall() external view returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_staticcall()).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_staticcall() external view returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_staticcall(rx); }
    function t_mem_addr_staticcall() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; (bool ok, bytes memory out) = address(ry).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_delegatecall() external  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_delegatecall() external  returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_delegatecall()).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_delegatecall() external  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_delegatecall(rx); }
    function t_mem_addr_delegatecall() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; (bool ok, bytes memory out) = address(ry).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_ext_call() external  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } IC(address(rx)).f(); return 1; }
    function t_retdirect_addr_ext_call() external  returns (uint256) { IC(address(_s_retdirect_addr_ext_call())).f(); return 1; }
    function t_param_addr_ext_call() external  returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_ext_call(rx); }
    function t_mem_addr_ext_call() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; IC(address(ry)).f(); return 1; }
    function t_local_addr_send() external payable returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } bool ok = rx.send(1); return ok ? 1 : 0; }
    function t_retdirect_addr_send() external payable returns (uint256) { bool ok = _s_retdirect_addr_send().send(1); return ok ? 1 : 0; }
    function t_param_addr_send() external payable returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_send(rx); }
    function t_mem_addr_send() external payable returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; bool ok = ry.send(1); return ok ? 1 : 0; }
    function t_local_addr_transfer() external payable returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } rx.transfer(1); return 1; }
    function t_retdirect_addr_transfer() external payable returns (uint256) { _s_retdirect_addr_transfer().transfer(1); return 1; }
    function t_param_addr_transfer() external payable returns (uint256) { address payable rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_transfer(rx); }
    function t_mem_addr_transfer() external payable returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } address payable ry = ra[0]; ry.transfer(1); return 1; }
}
contract Cctr_0 {
    function ping() external pure returns (uint256) { return 77; }
    receive() external payable {}
    function f() external {}
    function _s_retdirect_ext_arg() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg(IC pv) internal  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_arg_second(IC pv) internal  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f2(IC(address(0x1111111111111111111111111111111111111111)), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try(IC pv) internal  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_try_catch_arg(IC pv) internal  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_static(IC pv) internal  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_lowlevel_encode(IC pv) internal  returns (uint256) { Callee_ctr c = new Callee_ctr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ctr.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ext_new_arg(IC pv) internal  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ctor_arg(IC pv) internal  returns (uint256) { Ctor_ctr k = new Ctor_ctr(pv); uint256 r = k.got(); return r; }
    function _s_retdirect_addr_balance() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_balance(IC pv) internal  returns (uint256) { return address(pv).balance; }
    function _s_retdirect_addr_code_len() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_code_len(IC pv) internal view returns (uint256) { return address(pv).code.length == address(this).code.length ? 1 : 0; }
    function t_local_ext_arg() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r = c.f2(IC(address(0x1111111111111111111111111111111111111111)), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f2(IC(address(0x1111111111111111111111111111111111111111)), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r = c.f2(IC(address(0x1111111111111111111111111111111111111111)), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ctr.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_ctr c = new Callee_ctr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ctr.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ctr.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Callee_ctr c = new Callee_ctr(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_ctr c = new Callee_ctr(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Callee_ctr c = new Callee_ctr(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Ctor_ctr k = new Ctor_ctr(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_ctr k = new Ctor_ctr(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Ctor_ctr k = new Ctor_ctr(ry); uint256 r = k.got(); return r; }
    function t_local_addr_balance() external payable returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).balance; }
    function t_retdirect_addr_balance() external payable returns (uint256) { return address(_s_retdirect_addr_balance()).balance; }
    function t_param_addr_balance() external payable returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_balance(rx); }
    function t_mem_addr_balance() external payable returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; return address(ry).balance; }
    function t_local_addr_code_len() external view returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).code.length == address(this).code.length ? 1 : 0; }
    function t_retdirect_addr_code_len() external view returns (uint256) { return address(_s_retdirect_addr_code_len()).code.length == address(this).code.length ? 1 : 0; }
    function t_param_addr_code_len() external view returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_code_len(rx); }
    function t_mem_addr_code_len() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; return address(ry).code.length == address(this).code.length ? 1 : 0; }
}
contract Cctr_1 {
    function ping() external pure returns (uint256) { return 77; }
    receive() external payable {}
    function f() external {}
    function _s_retdirect_addr_codehash() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_codehash(IC pv) internal view returns (uint256) { return address(pv).codehash == address(this).codehash ? 1 : 0; }
    function _s_retdirect_addr_call() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_call(IC pv) internal  returns (uint256) { (bool ok, bytes memory out) = address(pv).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_staticcall() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_staticcall(IC pv) internal view returns (uint256) { (bool ok, bytes memory out) = address(pv).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_delegatecall() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_delegatecall(IC pv) internal  returns (uint256) { (bool ok, bytes memory out) = address(pv).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function _s_retdirect_addr_ext_call() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_addr_ext_call(IC pv) internal  returns (uint256) { IC(address(pv)).f(); return 1; }
    function _s_retdirect_ctr_ext_call() internal view returns (IC r) { assembly { r := or(address(), shl(160, 0xabcd)) } }
    function _p_param_ctr_ext_call(IC pv) internal  returns (uint256) { pv.f(); return 1; }
    function t_local_addr_codehash() external view returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return address(rx).codehash == address(this).codehash ? 1 : 0; }
    function t_retdirect_addr_codehash() external view returns (uint256) { return address(_s_retdirect_addr_codehash()).codehash == address(this).codehash ? 1 : 0; }
    function t_param_addr_codehash() external view returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_codehash(rx); }
    function t_mem_addr_codehash() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; return address(ry).codehash == address(this).codehash ? 1 : 0; }
    function t_local_addr_call() external  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_call() external  returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_call()).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_call() external  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_call(rx); }
    function t_mem_addr_call() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; (bool ok, bytes memory out) = address(ry).call(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_staticcall() external view returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_staticcall() external view returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_staticcall()).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_staticcall() external view returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_staticcall(rx); }
    function t_mem_addr_staticcall() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; (bool ok, bytes memory out) = address(ry).staticcall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_delegatecall() external  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } (bool ok, bytes memory out) = address(rx).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_retdirect_addr_delegatecall() external  returns (uint256) { (bool ok, bytes memory out) = address(_s_retdirect_addr_delegatecall()).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_param_addr_delegatecall() external  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_delegatecall(rx); }
    function t_mem_addr_delegatecall() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; (bool ok, bytes memory out) = address(ry).delegatecall(abi.encodeWithSignature("ping()")); return ok ? abi.decode(out, (uint256)) : 0; }
    function t_local_addr_ext_call() external  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } IC(address(rx)).f(); return 1; }
    function t_retdirect_addr_ext_call() external  returns (uint256) { IC(address(_s_retdirect_addr_ext_call())).f(); return 1; }
    function t_param_addr_ext_call() external  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_addr_ext_call(rx); }
    function t_mem_addr_ext_call() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; IC(address(ry)).f(); return 1; }
    function t_local_ctr_ext_call() external  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } rx.f(); return 1; }
    function t_retdirect_ctr_ext_call() external  returns (uint256) { _s_retdirect_ctr_ext_call().f(); return 1; }
    function t_param_ctr_ext_call() external  returns (uint256) { IC rx; assembly { rx := or(address(), shl(160, 0xabcd)) } return _p_param_ctr_ext_call(rx); }
    function t_mem_ctr_ext_call() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), or(address(), shl(160, 0xabcd))) } IC ry = ra[0]; ry.f(); return 1; }
}
contract Cbool_0 {
    function _s_retdirect_ext_arg() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_arg(bool pv) internal  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_arg_second(bool pv) internal  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f2(false, pv); return r; }
    function _s_retdirect_ext_try() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_try(bool pv) internal  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_try_catch_arg(bool pv) internal  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_static(bool pv) internal  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_lowlevel_encode(bool pv) internal  returns (uint256) { Callee_bool c = new Callee_bool(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_bool.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ext_new_arg(bool pv) internal  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ctor_arg(bool pv) internal  returns (uint256) { Ctor_bool k = new Ctor_bool(pv); uint256 r = k.got(); return r; }
    function t_local_ext_arg() external  returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r = c.f2(false, rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f2(false, _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r = c.f2(false, ry); return r; }
    function t_local_ext_try() external  returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_bool.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_bool c = new Callee_bool(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_bool.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_bool.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { bool rx; assembly { rx := 0x100 } Callee_bool c = new Callee_bool(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_bool c = new Callee_bool(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Callee_bool c = new Callee_bool(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { bool rx; assembly { rx := 0x100 } Ctor_bool k = new Ctor_bool(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_bool k = new Ctor_bool(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Ctor_bool k = new Ctor_bool(ry); uint256 r = k.got(); return r; }
}
contract Cb1_0 {
    function _s_retdirect_ext_arg() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_arg(bytes1 pv) internal  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_arg_second(bytes1 pv) internal  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f2(bytes1(0xcd), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_try(bytes1 pv) internal  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_try_catch_arg(bytes1 pv) internal  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_static(bytes1 pv) internal  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_lowlevel_encode(bytes1 pv) internal  returns (uint256) { Callee_b1 c = new Callee_b1(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b1.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ext_new_arg(bytes1 pv) internal  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ctor_arg(bytes1 pv) internal  returns (uint256) { Ctor_b1 k = new Ctor_b1(pv); uint256 r = k.got(); return r; }
    function t_local_ext_arg() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r = c.f2(bytes1(0xcd), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f2(bytes1(0xcd), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r = c.f2(bytes1(0xcd), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b1.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_b1 c = new Callee_b1(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b1.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b1.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Callee_b1 c = new Callee_b1(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_b1 c = new Callee_b1(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Callee_b1 c = new Callee_b1(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Ctor_b1 k = new Ctor_b1(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_b1 k = new Ctor_b1(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Ctor_b1 k = new Ctor_b1(ry); uint256 r = k.got(); return r; }
}
contract Cb4_0 {
    function _s_retdirect_ext_arg() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_arg(bytes4 pv) internal  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_arg_second(bytes4 pv) internal  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f2(bytes4(0x11223344), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_try(bytes4 pv) internal  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_try_catch_arg(bytes4 pv) internal  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_static(bytes4 pv) internal  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_lowlevel_encode(bytes4 pv) internal  returns (uint256) { Callee_b4 c = new Callee_b4(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b4.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ext_new_arg(bytes4 pv) internal  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ctor_arg(bytes4 pv) internal  returns (uint256) { Ctor_b4 k = new Ctor_b4(pv); uint256 r = k.got(); return r; }
    function t_local_ext_arg() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r = c.f2(bytes4(0x11223344), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f2(bytes4(0x11223344), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r = c.f2(bytes4(0x11223344), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b4.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_b4 c = new Callee_b4(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b4.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b4.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Callee_b4 c = new Callee_b4(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_b4 c = new Callee_b4(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Callee_b4 c = new Callee_b4(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Ctor_b4 k = new Ctor_b4(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_b4 k = new Ctor_b4(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Ctor_b4 k = new Ctor_b4(ry); uint256 r = k.got(); return r; }
}
contract Cb20_0 {
    function _s_retdirect_ext_arg() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_arg(bytes20 pv) internal  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_arg_second(bytes20 pv) internal  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f2(bytes20(0x1111111111111111111111111111111111111111), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_try(bytes20 pv) internal  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_try_catch_arg(bytes20 pv) internal  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_static(bytes20 pv) internal  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_lowlevel_encode(bytes20 pv) internal  returns (uint256) { Callee_b20 c = new Callee_b20(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b20.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ext_new_arg(bytes20 pv) internal  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ctor_arg(bytes20 pv) internal  returns (uint256) { Ctor_b20 k = new Ctor_b20(pv); uint256 r = k.got(); return r; }
    function t_local_ext_arg() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r = c.f2(bytes20(0x1111111111111111111111111111111111111111), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f2(bytes20(0x1111111111111111111111111111111111111111), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r = c.f2(bytes20(0x1111111111111111111111111111111111111111), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b20.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_b20 c = new Callee_b20(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b20.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_b20.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Callee_b20 c = new Callee_b20(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_b20 c = new Callee_b20(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Callee_b20 c = new Callee_b20(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Ctor_b20 k = new Ctor_b20(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_b20 k = new Ctor_b20(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Ctor_b20 k = new Ctor_b20(ry); uint256 r = k.got(); return r; }
}
contract Cen_0 {
    function _s_retdirect_ext_arg() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_arg(E pv) internal  returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_arg_second(E pv) internal  returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f2(E.A, pv); return r; }
    function _s_retdirect_ext_try() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_try(E pv) internal  returns (uint256) { Callee_en c = new Callee_en(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_try_catch_arg(E pv) internal  returns (uint256) { Callee_en c = new Callee_en(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_static(E pv) internal  returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_lowlevel_encode(E pv) internal  returns (uint256) { Callee_en c = new Callee_en(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_en.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ext_new_arg(E pv) internal  returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ctor_arg(E pv) internal  returns (uint256) { Ctor_en k = new Ctor_en(pv); uint256 r = k.got(); return r; }
    function t_local_ext_arg() external  returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r = c.f2(E.A, rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f2(E.A, _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r = c.f2(E.A, ry); return r; }
    function t_local_ext_try() external  returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_en c = new Callee_en(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_en c = new Callee_en(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_en.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_en c = new Callee_en(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_en.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_en.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { E rx; assembly { rx := 0x101 } Callee_en c = new Callee_en(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_en c = new Callee_en(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Callee_en c = new Callee_en(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { E rx; assembly { rx := 0x101 } Ctor_en k = new Ctor_en(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_en k = new Ctor_en(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Ctor_en k = new Ctor_en(ry); uint256 r = k.got(); return r; }
}
contract Cud_0 {
    function _s_retdirect_ext_arg() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_arg(U pv) internal  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_arg_second() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_arg_second(U pv) internal  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f2(U.wrap(3), pv); return r; }
    function _s_retdirect_ext_try() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_try(U pv) internal  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r; try c.f(pv) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function _s_retdirect_ext_try_catch_arg() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_try_catch_arg(U pv) internal  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r; try c.fail(pv) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function _s_retdirect_ext_static() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_static(U pv) internal  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ext_lowlevel_encode() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_lowlevel_encode(U pv) internal  returns (uint256) { Callee_ud c = new Callee_ud(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ud.f.selector, pv)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function _s_retdirect_ext_new_arg() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ext_new_arg(U pv) internal  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(pv); return r; }
    function _s_retdirect_ctor_arg() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ctor_arg(U pv) internal  returns (uint256) { Ctor_ud k = new Ctor_ud(pv); uint256 r = k.got(); return r; }
    function t_local_ext_arg() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_arg() external  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(_s_retdirect_ext_arg()); return r; }
    function t_param_ext_arg() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_arg(rx); }
    function t_mem_ext_arg() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r = c.f(ry); return r; }
    function t_local_ext_arg_second() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r = c.f2(U.wrap(3), rx); return r; }
    function t_retdirect_ext_arg_second() external  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f2(U.wrap(3), _s_retdirect_ext_arg_second()); return r; }
    function t_param_ext_arg_second() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_arg_second(rx); }
    function t_mem_ext_arg_second() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r = c.f2(U.wrap(3), ry); return r; }
    function t_local_ext_try() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r; try c.f(rx) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_retdirect_ext_try() external  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r; try c.f(_s_retdirect_ext_try()) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_param_ext_try() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_try(rx); }
    function t_mem_ext_try() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r; try c.f(ry) returns (uint256 v) { r = v; } catch { r = 999; } return r; }
    function t_local_ext_try_catch_arg() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r; try c.fail(rx) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_retdirect_ext_try_catch_arg() external  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r; try c.fail(_s_retdirect_ext_try_catch_arg()) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_param_ext_try_catch_arg() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_try_catch_arg(rx); }
    function t_mem_ext_try_catch_arg() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r; try c.fail(ry) { r = 1; } catch (bytes memory e) { r = e.length; } return r; }
    function t_local_ext_static() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_static() external  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(_s_retdirect_ext_static()); return r; }
    function t_param_ext_static() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_static(rx); }
    function t_mem_ext_static() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r = c.f(ry); return r; }
    function t_local_ext_lowlevel_encode() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ud.f.selector, rx)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_retdirect_ext_lowlevel_encode() external  returns (uint256) { Callee_ud c = new Callee_ud(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ud.f.selector, _s_retdirect_ext_lowlevel_encode())); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_param_ext_lowlevel_encode() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_lowlevel_encode(rx); }
    function t_mem_ext_lowlevel_encode() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); (bool ok, bytes memory out) = address(c).call(abi.encodeWithSelector(Callee_ud.f.selector, ry)); uint256 r = ok ? abi.decode(out, (uint256)) : 999; return r; }
    function t_local_ext_new_arg() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } Callee_ud c = new Callee_ud(); uint256 r = c.f(rx); return r; }
    function t_retdirect_ext_new_arg() external  returns (uint256) { Callee_ud c = new Callee_ud(); uint256 r = c.f(_s_retdirect_ext_new_arg()); return r; }
    function t_param_ext_new_arg() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ext_new_arg(rx); }
    function t_mem_ext_new_arg() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Callee_ud c = new Callee_ud(); uint256 r = c.f(ry); return r; }
    function t_local_ctor_arg() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } Ctor_ud k = new Ctor_ud(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external  returns (uint256) { Ctor_ud k = new Ctor_ud(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Ctor_ud k = new Ctor_ud(ry); uint256 r = k.got(); return r; }
}
