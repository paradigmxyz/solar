//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_marray_elem => 27
//@ run-call: Cu8_0::t_retdirect_marray_elem => 27
//@ run-call: Cu8_0::t_param_marray_elem => 27
//@ run-call: Cu8_0::t_mem_marray_elem => 27
//@ run-call: Cu8_0::t_local_marray_fixed => 27
//@ run-call: Cu8_0::t_retdirect_marray_fixed => 27
//@ run-call: Cu8_0::t_param_marray_fixed => 27
//@ run-call: Cu8_0::t_mem_marray_fixed => 27
//@ run-call: Cu8_0::t_local_marray_literal => 27
//@ run-call: Cu8_0::t_retdirect_marray_literal => 27
//@ run-call: Cu8_0::t_param_marray_literal => 27
//@ run-call: Cu8_0::t_mem_marray_literal => 27
//@ run-call: Cu8_0::t_local_marray_literal_dyn => 27
//@ run-call: Cu8_0::t_retdirect_marray_literal_dyn => 27
//@ run-call: Cu8_0::t_param_marray_literal_dyn => 27
//@ run-call: Cu8_0::t_mem_marray_literal_dyn => 27
//@ run-call: Cu8_0::t_local_mstruct_field => 27
//@ run-call: Cu8_0::t_retdirect_mstruct_field => 27
//@ run-call: Cu8_0::t_param_mstruct_field => 27
//@ run-call: Cu8_0::t_mem_mstruct_field => 27
//@ run-call: Cu8_0::t_local_mstruct_literal => 27
//@ run-call: Cu8_0::t_retdirect_mstruct_literal => 27
//@ run-call: Cu8_0::t_param_mstruct_literal => 27
//@ run-call: Cu8_0::t_mem_mstruct_literal => 27
//@ run-call: Cu8_0::t_local_mnested => 27
//@ run-call: Cu8_0::t_retdirect_mnested => 27
//@ run-call: Cu8_0::t_param_mnested => 27
//@ run-call: Cu8_0::t_mem_mnested => 27
//@ run-call: Cu8_0::t_local_storage_scalar => 27
//@ run-call: Cu8_0::t_retdirect_storage_scalar => 27
//@ run-call: Cu8_0::t_param_storage_scalar => 27
//@ run-call: Cu8_0::t_mem_storage_scalar => 27
//@ run-call: Cu8_0::t_local_storage_packed => 2235153
//@ run-call: Cu8_0::t_retdirect_storage_packed => 2235153
//@ run-call: Cu8_0::t_param_storage_packed => 2235153
//@ run-call: Cu8_0::t_mem_storage_packed => 2235153
//@ run-call: Cu8_0::t_local_storage_struct_copy => 27
//@ run-call: Cu8_0::t_retdirect_storage_struct_copy => 27
//@ run-call: Cu8_0::t_param_storage_struct_copy => 27
//@ run-call: Cu8_0::t_mem_storage_struct_copy => 27
//@ run-call: Cu8_1::t_local_storage_push => 27
//@ run-call: Cu8_1::t_retdirect_storage_push => 27
//@ run-call: Cu8_1::t_param_storage_push => 27
//@ run-call: Cu8_1::t_mem_storage_push => 27
//@ run-call: Cu8_1::t_local_storage_array_elem => 27
//@ run-call: Cu8_1::t_retdirect_storage_array_elem => 27
//@ run-call: Cu8_1::t_param_storage_array_elem => 27
//@ run-call: Cu8_1::t_mem_storage_array_elem => 27
//@ run-call: Cu8_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Cu8_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cu8_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Cu8_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cu8_1::t_local_mapping_key_read => 9
//@ run-call: Cu8_1::t_retdirect_mapping_key_read => 9
//@ run-call: Cu8_1::t_param_mapping_key_read => 9
//@ run-call: Cu8_1::t_mem_mapping_key_read => 9
//@ run-call: Cu8_1::t_local_mapping_value_raw => 27
//@ run-call: Cu8_1::t_retdirect_mapping_value_raw => 27
//@ run-call: Cu8_1::t_param_mapping_value_raw => 27
//@ run-call: Cu8_1::t_mem_mapping_value_raw => 27
//@ run-call: Cu8_1::t_local_marray_index => 70
//@ run-call: Cu8_1::t_retdirect_marray_index => 70
//@ run-call: Cu8_1::t_param_marray_index => 70
//@ run-call: Cu8_1::t_mem_marray_index => 70
//@ run-call: Cu8_1::t_local_marray_index_store => 71
//@ run-call: Cu8_1::t_retdirect_marray_index_store => 71
//@ run-call: Cu8_1::t_param_marray_index_store => 71
//@ run-call: Cu8_1::t_mem_marray_index_store => 71
//@ run-call-fail: Cu8_1::t_local_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_1::t_retdirect_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_1::t_param_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu8_1::t_mem_sarray_index => 270
//@ run-call-fail: Cu8_1::t_local_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_1::t_retdirect_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_1::t_param_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu8_1::t_mem_sarray_index_store => 72
//@ run-call-fail: Cu8_1::t_local_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_1::t_retdirect_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_1::t_param_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu8_1::t_mem_sfixed_index => 73
//@ run-call: Cu8_2::t_local_bytes_index => 68
//@ run-call: Cu8_2::t_retdirect_bytes_index => 68
//@ run-call: Cu8_2::t_param_bytes_index => 68
//@ run-call: Cu8_2::t_mem_bytes_index => 68
//@ run-call: Cu8_2::t_local_b32_index => 28
//@ run-call: Cu8_2::t_retdirect_b32_index => 28
//@ run-call: Cu8_2::t_param_b32_index => 28
//@ run-call: Cu8_2::t_mem_b32_index => 28
//@ run-call: Cu8_2::t_local_new_array_len => 27
//@ run-call: Cu8_2::t_retdirect_new_array_len => 27
//@ run-call: Cu8_2::t_param_new_array_len => 27
//@ run-call: Cu8_2::t_mem_new_array_len => 27
//@ run-call: Cu8_2::t_local_new_bytes_len => 27
//@ run-call: Cu8_2::t_retdirect_new_bytes_len => 27
//@ run-call: Cu8_2::t_param_new_bytes_len => 27
//@ run-call: Cu8_2::t_mem_new_bytes_len => 27
//@ run-call: Cu8_2::t_local_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu8_2::t_retdirect_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu8_2::t_mem_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu8_2::t_local_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu8_2::t_retdirect_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu8_2::t_mem_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu8_2::t_local_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu8_2::t_retdirect_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu8_2::t_mem_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu8_2::t_local_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu8_2::t_retdirect_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu8_2::t_mem_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu8_2::t_local_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu8_2::t_retdirect_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu8_2::t_mem_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu8_2::t_local_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu8_2::t_retdirect_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu8_2::t_mem_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_0::t_local_marray_elem => 27
//@ run-call: Cu64_0::t_retdirect_marray_elem => 27
//@ run-call: Cu64_0::t_param_marray_elem => 27
//@ run-call: Cu64_0::t_mem_marray_elem => 27
//@ run-call: Cu64_0::t_local_marray_fixed => 27
//@ run-call: Cu64_0::t_retdirect_marray_fixed => 27
//@ run-call: Cu64_0::t_param_marray_fixed => 27
//@ run-call: Cu64_0::t_mem_marray_fixed => 27
//@ run-call: Cu64_0::t_local_marray_literal => 27
//@ run-call: Cu64_0::t_retdirect_marray_literal => 27
//@ run-call: Cu64_0::t_param_marray_literal => 27
//@ run-call: Cu64_0::t_mem_marray_literal => 27
//@ run-call: Cu64_0::t_local_marray_literal_dyn => 27
//@ run-call: Cu64_0::t_retdirect_marray_literal_dyn => 27
//@ run-call: Cu64_0::t_param_marray_literal_dyn => 27
//@ run-call: Cu64_0::t_mem_marray_literal_dyn => 27
//@ run-call: Cu64_0::t_local_mstruct_field => 27
//@ run-call: Cu64_0::t_retdirect_mstruct_field => 27
//@ run-call: Cu64_0::t_param_mstruct_field => 27
//@ run-call: Cu64_0::t_mem_mstruct_field => 27
//@ run-call: Cu64_0::t_local_mstruct_literal => 27
//@ run-call: Cu64_0::t_retdirect_mstruct_literal => 27
//@ run-call: Cu64_0::t_param_mstruct_literal => 27
//@ run-call: Cu64_0::t_mem_mstruct_literal => 27
//@ run-call: Cu64_0::t_local_mnested => 27
//@ run-call: Cu64_0::t_retdirect_mnested => 27
//@ run-call: Cu64_0::t_param_mnested => 27
//@ run-call: Cu64_0::t_mem_mnested => 27
//@ run-call: Cu64_0::t_local_storage_scalar => 27
//@ run-call: Cu64_0::t_retdirect_storage_scalar => 27
//@ run-call: Cu64_0::t_param_storage_scalar => 27
//@ run-call: Cu64_0::t_mem_storage_scalar => 27
//@ run-call: Cu64_0::t_local_storage_packed => 160560460417567937272593
//@ run-call: Cu64_0::t_retdirect_storage_packed => 160560460417567937272593
//@ run-call: Cu64_0::t_param_storage_packed => 160560460417567937272593
//@ run-call: Cu64_0::t_mem_storage_packed => 160560460417567937272593
//@ run-call: Cu64_0::t_local_storage_struct_copy => 27
//@ run-call: Cu64_0::t_retdirect_storage_struct_copy => 27
//@ run-call: Cu64_0::t_param_storage_struct_copy => 27
//@ run-call: Cu64_0::t_mem_storage_struct_copy => 27
//@ run-call: Cu64_1::t_local_storage_push => 27
//@ run-call: Cu64_1::t_retdirect_storage_push => 27
//@ run-call: Cu64_1::t_param_storage_push => 27
//@ run-call: Cu64_1::t_mem_storage_push => 27
//@ run-call: Cu64_1::t_local_storage_array_elem => 27
//@ run-call: Cu64_1::t_retdirect_storage_array_elem => 27
//@ run-call: Cu64_1::t_param_storage_array_elem => 27
//@ run-call: Cu64_1::t_mem_storage_array_elem => 27
//@ run-call: Cu64_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Cu64_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cu64_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Cu64_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cu64_1::t_local_mapping_key_read => 9
//@ run-call: Cu64_1::t_retdirect_mapping_key_read => 9
//@ run-call: Cu64_1::t_param_mapping_key_read => 9
//@ run-call: Cu64_1::t_mem_mapping_key_read => 9
//@ run-call: Cu64_1::t_local_mapping_value_raw => 27
//@ run-call: Cu64_1::t_retdirect_mapping_value_raw => 27
//@ run-call: Cu64_1::t_param_mapping_value_raw => 27
//@ run-call: Cu64_1::t_mem_mapping_value_raw => 27
//@ run-call: Cu64_1::t_local_marray_index => 70
//@ run-call: Cu64_1::t_retdirect_marray_index => 70
//@ run-call: Cu64_1::t_param_marray_index => 70
//@ run-call: Cu64_1::t_mem_marray_index => 70
//@ run-call: Cu64_1::t_local_marray_index_store => 71
//@ run-call: Cu64_1::t_retdirect_marray_index_store => 71
//@ run-call: Cu64_1::t_param_marray_index_store => 71
//@ run-call: Cu64_1::t_mem_marray_index_store => 71
//@ run-call-fail: Cu64_1::t_local_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_1::t_retdirect_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_1::t_param_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu64_1::t_mem_sarray_index => 270
//@ run-call-fail: Cu64_1::t_local_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_1::t_retdirect_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_1::t_param_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu64_1::t_mem_sarray_index_store => 72
//@ run-call-fail: Cu64_1::t_local_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_1::t_retdirect_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_1::t_param_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu64_1::t_mem_sfixed_index => 73
//@ run-call: Cu64_2::t_local_bytes_index => 68
//@ run-call: Cu64_2::t_retdirect_bytes_index => 68
//@ run-call: Cu64_2::t_param_bytes_index => 68
//@ run-call: Cu64_2::t_mem_bytes_index => 68
//@ run-call: Cu64_2::t_local_b32_index => 28
//@ run-call: Cu64_2::t_retdirect_b32_index => 28
//@ run-call: Cu64_2::t_param_b32_index => 28
//@ run-call: Cu64_2::t_mem_b32_index => 28
//@ run-call: Cu64_2::t_local_new_array_len => 27
//@ run-call: Cu64_2::t_retdirect_new_array_len => 27
//@ run-call: Cu64_2::t_param_new_array_len => 27
//@ run-call: Cu64_2::t_mem_new_array_len => 27
//@ run-call: Cu64_2::t_local_new_bytes_len => 27
//@ run-call: Cu64_2::t_retdirect_new_bytes_len => 27
//@ run-call: Cu64_2::t_param_new_bytes_len => 27
//@ run-call: Cu64_2::t_mem_new_bytes_len => 27
//@ run-call: Cu64_2::t_local_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_2::t_retdirect_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_2::t_mem_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_2::t_local_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu64_2::t_retdirect_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu64_2::t_mem_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu64_2::t_local_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu64_2::t_retdirect_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu64_2::t_mem_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu64_2::t_local_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu64_2::t_retdirect_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu64_2::t_mem_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu64_2::t_local_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu64_2::t_retdirect_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu64_2::t_mem_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu64_2::t_local_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_2::t_retdirect_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_2::t_mem_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Ci8_0::t_local_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_storage_scalar => 253
//@ run-call: Ci8_0::t_retdirect_storage_scalar => 253
//@ run-call: Ci8_0::t_param_storage_scalar => 253
//@ run-call: Ci8_0::t_mem_storage_scalar => 253
//@ run-call: Ci8_0::t_local_storage_packed => 2293009
//@ run-call: Ci8_0::t_retdirect_storage_packed => 2293009
//@ run-call: Ci8_0::t_param_storage_packed => 2293009
//@ run-call: Ci8_0::t_mem_storage_packed => 2293009
//@ run-call: Ci8_0::t_local_storage_struct_copy => 253
//@ run-call: Ci8_0::t_retdirect_storage_struct_copy => 253
//@ run-call: Ci8_0::t_param_storage_struct_copy => 253
//@ run-call: Ci8_0::t_mem_storage_struct_copy => 253
//@ run-call: Ci8_1::t_local_storage_push => 253
//@ run-call: Ci8_1::t_retdirect_storage_push => 253
//@ run-call: Ci8_1::t_param_storage_push => 253
//@ run-call: Ci8_1::t_mem_storage_push => 253
//@ run-call: Ci8_1::t_local_storage_array_elem => 253
//@ run-call: Ci8_1::t_retdirect_storage_array_elem => 253
//@ run-call: Ci8_1::t_param_storage_array_elem => 253
//@ run-call: Ci8_1::t_mem_storage_array_elem => 253
//@ run-call: Ci8_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Ci8_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Ci8_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Ci8_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Ci8_1::t_local_mapping_key_read => 9
//@ run-call: Ci8_1::t_retdirect_mapping_key_read => 9
//@ run-call: Ci8_1::t_param_mapping_key_read => 9
//@ run-call: Ci8_1::t_mem_mapping_key_read => 9
//@ run-call: Ci8_1::t_local_mapping_value_raw => 253
//@ run-call: Ci8_1::t_retdirect_mapping_value_raw => 253
//@ run-call: Ci8_1::t_param_mapping_value_raw => 253
//@ run-call: Ci8_1::t_mem_mapping_value_raw => 253
//@ run-call: Ci24_0::t_local_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_storage_scalar => 16777213
//@ run-call: Ci24_0::t_retdirect_storage_scalar => 16777213
//@ run-call: Ci24_0::t_param_storage_scalar => 16777213
//@ run-call: Ci24_0::t_mem_storage_scalar => 16777213
//@ run-call: Ci24_0::t_local_storage_packed => 150323854609
//@ run-call: Ci24_0::t_retdirect_storage_packed => 150323854609
//@ run-call: Ci24_0::t_param_storage_packed => 150323854609
//@ run-call: Ci24_0::t_mem_storage_packed => 150323854609
//@ run-call: Ci24_0::t_local_storage_struct_copy => 16777213
//@ run-call: Ci24_0::t_retdirect_storage_struct_copy => 16777213
//@ run-call: Ci24_0::t_param_storage_struct_copy => 16777213
//@ run-call: Ci24_0::t_mem_storage_struct_copy => 16777213
//@ run-call: Ci24_1::t_local_storage_push => 16777213
//@ run-call: Ci24_1::t_retdirect_storage_push => 16777213
//@ run-call: Ci24_1::t_param_storage_push => 16777213
//@ run-call: Ci24_1::t_mem_storage_push => 16777213
//@ run-call: Ci24_1::t_local_storage_array_elem => 16777213
//@ run-call: Ci24_1::t_retdirect_storage_array_elem => 16777213
//@ run-call: Ci24_1::t_param_storage_array_elem => 16777213
//@ run-call: Ci24_1::t_mem_storage_array_elem => 16777213
//@ run-call: Ci24_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Ci24_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Ci24_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Ci24_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Ci24_1::t_local_mapping_key_read => 9
//@ run-call: Ci24_1::t_retdirect_mapping_key_read => 9
//@ run-call: Ci24_1::t_param_mapping_key_read => 9
//@ run-call: Ci24_1::t_mem_mapping_key_read => 9
//@ run-call: Ci24_1::t_local_mapping_value_raw => 16777213
//@ run-call: Ci24_1::t_retdirect_mapping_value_raw => 16777213
//@ run-call: Ci24_1::t_param_mapping_value_raw => 16777213
//@ run-call: Ci24_1::t_mem_mapping_value_raw => 16777213
//@ run-call: Caddr_0::t_local_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddr_0::t_retdirect_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddr_0::t_param_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddr_0::t_mem_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddr_0::t_local_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_local_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_retdirect_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_param_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_mem_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_local_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_retdirect_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_param_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_mem_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Caddr_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Caddr_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Caddr_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Caddr_1::t_local_mapping_key_read => 9
//@ run-call: Caddr_1::t_retdirect_mapping_key_read => 9
//@ run-call: Caddr_1::t_param_mapping_key_read => 9
//@ run-call: Caddr_1::t_mem_mapping_key_read => 9
//@ run-call: Caddr_1::t_local_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_retdirect_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_param_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_1::t_mem_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddrp_0::t_retdirect_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddrp_0::t_param_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddrp_0::t_mem_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddrp_0::t_local_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_local_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_retdirect_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_param_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_mem_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_local_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_retdirect_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_param_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_mem_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Caddrp_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Caddrp_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Caddrp_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Caddrp_1::t_local_mapping_key_read => 9
//@ run-call: Caddrp_1::t_retdirect_mapping_key_read => 9
//@ run-call: Caddrp_1::t_param_mapping_key_read => 9
//@ run-call: Caddrp_1::t_mem_mapping_key_read => 9
//@ run-call: Caddrp_1::t_local_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_retdirect_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_param_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_1::t_mem_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_marray_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_marray_fixed => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_marray_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_marray_literal_dyn => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_mnested => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cctr_0::t_retdirect_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cctr_0::t_param_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cctr_0::t_mem_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cctr_0::t_local_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_local_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_retdirect_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_param_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_mem_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_local_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_retdirect_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_param_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_mem_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Cctr_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cctr_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Cctr_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cctr_1::t_local_mapping_key_read => 9
//@ run-call: Cctr_1::t_retdirect_mapping_key_read => 9
//@ run-call: Cctr_1::t_param_mapping_key_read => 9
//@ run-call: Cctr_1::t_mem_mapping_key_read => 9
//@ run-call: Cctr_1::t_local_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_retdirect_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_param_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_1::t_mem_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cbool_0::t_local_marray_elem => 1
//@ run-call: Cbool_0::t_retdirect_marray_elem => 1
//@ run-call: Cbool_0::t_param_marray_elem => 1
//@ run-call: Cbool_0::t_mem_marray_elem => 1
//@ run-call: Cbool_0::t_local_marray_fixed => 1
//@ run-call: Cbool_0::t_retdirect_marray_fixed => 1
//@ run-call: Cbool_0::t_param_marray_fixed => 1
//@ run-call: Cbool_0::t_mem_marray_fixed => 1
//@ run-call: Cbool_0::t_local_marray_literal => 1
//@ run-call: Cbool_0::t_retdirect_marray_literal => 1
//@ run-call: Cbool_0::t_param_marray_literal => 1
//@ run-call: Cbool_0::t_mem_marray_literal => 1
//@ run-call: Cbool_0::t_local_marray_literal_dyn => 1
//@ run-call: Cbool_0::t_retdirect_marray_literal_dyn => 1
//@ run-call: Cbool_0::t_param_marray_literal_dyn => 1
//@ run-call: Cbool_0::t_mem_marray_literal_dyn => 1
//@ run-call: Cbool_0::t_local_mstruct_field => 1
//@ run-call: Cbool_0::t_retdirect_mstruct_field => 1
//@ run-call: Cbool_0::t_param_mstruct_field => 1
//@ run-call: Cbool_0::t_mem_mstruct_field => 1
//@ run-call: Cbool_0::t_local_mstruct_literal => 1
//@ run-call: Cbool_0::t_retdirect_mstruct_literal => 1
//@ run-call: Cbool_0::t_param_mstruct_literal => 1
//@ run-call: Cbool_0::t_mem_mstruct_literal => 1
//@ run-call: Cbool_0::t_local_mnested => 1
//@ run-call: Cbool_0::t_retdirect_mnested => 1
//@ run-call: Cbool_0::t_param_mnested => 1
//@ run-call: Cbool_0::t_mem_mnested => 1
//@ run-call: Cbool_0::t_local_storage_scalar => 1
//@ run-call: Cbool_0::t_retdirect_storage_scalar => 1
//@ run-call: Cbool_0::t_param_storage_scalar => 1
//@ run-call: Cbool_0::t_mem_storage_scalar => 1
//@ run-call: Cbool_0::t_local_storage_packed => 2228497
//@ run-call: Cbool_0::t_retdirect_storage_packed => 2228497
//@ run-call: Cbool_0::t_param_storage_packed => 2228497
//@ run-call: Cbool_0::t_mem_storage_packed => 2228497
//@ run-call: Cbool_0::t_local_storage_struct_copy => 1
//@ run-call: Cbool_0::t_retdirect_storage_struct_copy => 1
//@ run-call: Cbool_0::t_param_storage_struct_copy => 1
//@ run-call: Cbool_0::t_mem_storage_struct_copy => 1
//@ run-call: Cbool_1::t_local_storage_push => 1
//@ run-call: Cbool_1::t_retdirect_storage_push => 1
//@ run-call: Cbool_1::t_param_storage_push => 1
//@ run-call: Cbool_1::t_mem_storage_push => 1
//@ run-call: Cbool_1::t_local_storage_array_elem => 1
//@ run-call: Cbool_1::t_retdirect_storage_array_elem => 1
//@ run-call: Cbool_1::t_param_storage_array_elem => 1
//@ run-call: Cbool_1::t_mem_storage_array_elem => 1
//@ run-call: Cbool_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Cbool_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cbool_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Cbool_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cbool_1::t_local_mapping_key_read => 9
//@ run-call: Cbool_1::t_retdirect_mapping_key_read => 9
//@ run-call: Cbool_1::t_param_mapping_key_read => 9
//@ run-call: Cbool_1::t_mem_mapping_key_read => 9
//@ run-call: Cbool_1::t_local_mapping_value_raw => 1
//@ run-call: Cbool_1::t_retdirect_mapping_value_raw => 1
//@ run-call: Cbool_1::t_param_mapping_value_raw => 1
//@ run-call: Cbool_1::t_mem_mapping_value_raw => 1
//@ run-call: Cb1_0::t_local_marray_elem => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_marray_elem => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_marray_elem => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_marray_elem => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_marray_fixed => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_marray_fixed => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_marray_fixed => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_marray_fixed => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_marray_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_marray_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_marray_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_marray_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_marray_literal_dyn => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_marray_literal_dyn => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_marray_literal_dyn => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_marray_literal_dyn => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_mstruct_field => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_mstruct_field => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_mstruct_field => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_mstruct_field => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_mstruct_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_mstruct_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_mstruct_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_mstruct_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_mnested => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_mnested => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_mnested => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_mnested => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_storage_scalar => 171
//@ run-call: Cb1_0::t_retdirect_storage_scalar => 171
//@ run-call: Cb1_0::t_param_storage_scalar => 171
//@ run-call: Cb1_0::t_mem_storage_scalar => 171
//@ run-call: Cb1_0::t_local_storage_packed => 2272017
//@ run-call: Cb1_0::t_retdirect_storage_packed => 2272017
//@ run-call: Cb1_0::t_param_storage_packed => 2272017
//@ run-call: Cb1_0::t_mem_storage_packed => 2272017
//@ run-call: Cb1_0::t_local_storage_struct_copy => 171
//@ run-call: Cb1_0::t_retdirect_storage_struct_copy => 171
//@ run-call: Cb1_0::t_param_storage_struct_copy => 171
//@ run-call: Cb1_0::t_mem_storage_struct_copy => 171
//@ run-call: Cb1_1::t_local_storage_push => 171
//@ run-call: Cb1_1::t_retdirect_storage_push => 171
//@ run-call: Cb1_1::t_param_storage_push => 171
//@ run-call: Cb1_1::t_mem_storage_push => 171
//@ run-call: Cb1_1::t_local_storage_array_elem => 171
//@ run-call: Cb1_1::t_retdirect_storage_array_elem => 171
//@ run-call: Cb1_1::t_param_storage_array_elem => 171
//@ run-call: Cb1_1::t_mem_storage_array_elem => 171
//@ run-call: Cb1_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Cb1_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cb1_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Cb1_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cb1_1::t_local_mapping_key_read => 9
//@ run-call: Cb1_1::t_retdirect_mapping_key_read => 9
//@ run-call: Cb1_1::t_param_mapping_key_read => 9
//@ run-call: Cb1_1::t_mem_mapping_key_read => 9
//@ run-call: Cb1_1::t_local_mapping_value_raw => 171
//@ run-call: Cb1_1::t_retdirect_mapping_value_raw => 171
//@ run-call: Cb1_1::t_param_mapping_value_raw => 171
//@ run-call: Cb1_1::t_mem_mapping_value_raw => 171
//@ run-call: Cb1_1::t_local_storage_bytes_push => 77345497107738552411838431392522000948863935069627095510731433067785723314178
//@ run-call: Cb1_1::t_retdirect_storage_bytes_push => 77345497107738552411838431392522000948863935069627095510731433067785723314178
//@ run-call: Cb1_1::t_param_storage_bytes_push => 77345497107738552411838431392522000948863935069627095510731433067785723314178
//@ run-call: Cb1_1::t_mem_storage_bytes_push => 77345497107738552411838431392522000948863935069627095510731433067785723314178
//@ run-call: Cb1_1::t_local_bytes_to_storage_bytesN => 171
//@ run-call: Cb1_1::t_retdirect_bytes_to_storage_bytesN => 171
//@ run-call: Cb1_1::t_param_bytes_to_storage_bytesN => 171
//@ run-call: Cb1_1::t_mem_bytes_to_storage_bytesN => 171
//@ run-call: Cb4_0::t_local_marray_elem => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_marray_elem => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_marray_elem => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_marray_elem => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_marray_fixed => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_marray_fixed => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_marray_fixed => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_marray_fixed => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_marray_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_marray_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_marray_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_marray_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_marray_literal_dyn => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_marray_literal_dyn => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_marray_literal_dyn => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_marray_literal_dyn => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_mstruct_field => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_mstruct_field => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_mstruct_field => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_mstruct_field => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_mstruct_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_mstruct_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_mstruct_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_mstruct_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_mnested => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_mnested => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_mnested => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_mnested => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_storage_scalar => 3735928559
//@ run-call: Cb4_0::t_retdirect_storage_scalar => 3735928559
//@ run-call: Cb4_0::t_param_storage_scalar => 3735928559
//@ run-call: Cb4_0::t_mem_storage_scalar => 3735928559
//@ run-call: Cb4_0::t_local_storage_packed => 38339793055505
//@ run-call: Cb4_0::t_retdirect_storage_packed => 38339793055505
//@ run-call: Cb4_0::t_param_storage_packed => 38339793055505
//@ run-call: Cb4_0::t_mem_storage_packed => 38339793055505
//@ run-call: Cb4_0::t_local_storage_struct_copy => 3735928559
//@ run-call: Cb4_0::t_retdirect_storage_struct_copy => 3735928559
//@ run-call: Cb4_0::t_param_storage_struct_copy => 3735928559
//@ run-call: Cb4_0::t_mem_storage_struct_copy => 3735928559
//@ run-call: Cb4_1::t_local_storage_push => 3735928559
//@ run-call: Cb4_1::t_retdirect_storage_push => 3735928559
//@ run-call: Cb4_1::t_param_storage_push => 3735928559
//@ run-call: Cb4_1::t_mem_storage_push => 3735928559
//@ run-call: Cb4_1::t_local_storage_array_elem => 3735928559
//@ run-call: Cb4_1::t_retdirect_storage_array_elem => 3735928559
//@ run-call: Cb4_1::t_param_storage_array_elem => 3735928559
//@ run-call: Cb4_1::t_mem_storage_array_elem => 3735928559
//@ run-call: Cb4_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Cb4_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cb4_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Cb4_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cb4_1::t_local_mapping_key_read => 9
//@ run-call: Cb4_1::t_retdirect_mapping_key_read => 9
//@ run-call: Cb4_1::t_param_mapping_key_read => 9
//@ run-call: Cb4_1::t_mem_mapping_key_read => 9
//@ run-call: Cb4_1::t_local_mapping_value_raw => 3735928559
//@ run-call: Cb4_1::t_retdirect_mapping_value_raw => 3735928559
//@ run-call: Cb4_1::t_param_mapping_value_raw => 3735928559
//@ run-call: Cb4_1::t_mem_mapping_value_raw => 3735928559
//@ run-call: Cb4_1::t_local_bytes_to_storage_bytesN => 3735928559
//@ run-call: Cb4_1::t_retdirect_bytes_to_storage_bytesN => 3735928559
//@ run-call: Cb4_1::t_param_bytes_to_storage_bytesN => 3735928559
//@ run-call: Cb4_1::t_mem_bytes_to_storage_bytesN => 3735928559
//@ run-call: Cb20_0::t_local_marray_elem => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_marray_elem => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_marray_elem => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_marray_elem => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_marray_fixed => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_marray_fixed => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_marray_fixed => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_marray_fixed => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_marray_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_marray_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_marray_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_marray_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_marray_literal_dyn => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_marray_literal_dyn => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_marray_literal_dyn => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_marray_literal_dyn => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_mstruct_field => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_mstruct_field => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_mstruct_field => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_mstruct_field => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_mstruct_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_mstruct_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_mstruct_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_mstruct_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_mnested => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_mnested => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_mnested => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_mnested => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_local_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cb20_0::t_retdirect_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cb20_0::t_param_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cb20_0::t_mem_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cb20_0::t_local_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_local_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_retdirect_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_param_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_mem_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_local_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_retdirect_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_param_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_mem_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Cb20_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cb20_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Cb20_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cb20_1::t_local_mapping_key_read => 9
//@ run-call: Cb20_1::t_retdirect_mapping_key_read => 9
//@ run-call: Cb20_1::t_param_mapping_key_read => 9
//@ run-call: Cb20_1::t_mem_mapping_key_read => 9
//@ run-call: Cb20_1::t_local_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_retdirect_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_param_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_mem_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_local_bytes_to_storage_bytesN => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_retdirect_bytes_to_storage_bytesN => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_param_bytes_to_storage_bytesN => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_1::t_mem_bytes_to_storage_bytesN => 103929005307130220006098923584552504982110632080
//@ run-call-fail: Cen_0::t_local_marray_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_marray_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_marray_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_marray_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_marray_fixed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_marray_fixed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_marray_fixed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_marray_fixed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_marray_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_marray_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_marray_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_marray_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_marray_literal_dyn => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_marray_literal_dyn => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_marray_literal_dyn => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_marray_literal_dyn => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_mstruct_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_mstruct_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_mstruct_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_mstruct_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_mstruct_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_mstruct_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_mstruct_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_mstruct_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_mnested => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_mnested => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_mnested => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_mnested => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_storage_scalar => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_storage_scalar => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_storage_scalar => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_storage_scalar => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_storage_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_storage_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_storage_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_storage_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_storage_struct_copy => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_storage_struct_copy => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_storage_struct_copy => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_storage_struct_copy => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_local_storage_push => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_retdirect_storage_push => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_param_storage_push => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_mem_storage_push => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_local_storage_array_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_retdirect_storage_array_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_param_storage_array_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_mem_storage_array_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_local_mapping_value_key_dirty => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_retdirect_mapping_value_key_dirty => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_param_mapping_value_key_dirty => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_mem_mapping_value_key_dirty => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_local_mapping_key_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_retdirect_mapping_key_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_param_mapping_key_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_mem_mapping_key_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_local_mapping_value_raw => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_retdirect_mapping_value_raw => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_param_mapping_value_raw => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_1::t_mem_mapping_value_raw => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_marray_elem => 27
//@ run-call: Cud_0::t_retdirect_marray_elem => 27
//@ run-call: Cud_0::t_param_marray_elem => 27
//@ run-call: Cud_0::t_mem_marray_elem => 27
//@ run-call: Cud_0::t_local_marray_fixed => 27
//@ run-call: Cud_0::t_retdirect_marray_fixed => 27
//@ run-call: Cud_0::t_param_marray_fixed => 27
//@ run-call: Cud_0::t_mem_marray_fixed => 27
//@ run-call: Cud_0::t_local_marray_literal => 27
//@ run-call: Cud_0::t_retdirect_marray_literal => 27
//@ run-call: Cud_0::t_param_marray_literal => 27
//@ run-call: Cud_0::t_mem_marray_literal => 27
//@ run-call: Cud_0::t_local_marray_literal_dyn => 27
//@ run-call: Cud_0::t_retdirect_marray_literal_dyn => 27
//@ run-call: Cud_0::t_param_marray_literal_dyn => 27
//@ run-call: Cud_0::t_mem_marray_literal_dyn => 27
//@ run-call: Cud_0::t_local_mstruct_field => 27
//@ run-call: Cud_0::t_retdirect_mstruct_field => 27
//@ run-call: Cud_0::t_param_mstruct_field => 27
//@ run-call: Cud_0::t_mem_mstruct_field => 27
//@ run-call: Cud_0::t_local_mstruct_literal => 27
//@ run-call: Cud_0::t_retdirect_mstruct_literal => 27
//@ run-call: Cud_0::t_param_mstruct_literal => 27
//@ run-call: Cud_0::t_mem_mstruct_literal => 27
//@ run-call: Cud_0::t_local_mnested => 27
//@ run-call: Cud_0::t_retdirect_mnested => 27
//@ run-call: Cud_0::t_param_mnested => 27
//@ run-call: Cud_0::t_mem_mnested => 27
//@ run-call: Cud_0::t_local_storage_scalar => 27
//@ run-call: Cud_0::t_retdirect_storage_scalar => 27
//@ run-call: Cud_0::t_param_storage_scalar => 27
//@ run-call: Cud_0::t_mem_storage_scalar => 27
//@ run-call: Cud_0::t_local_storage_packed => 2235153
//@ run-call: Cud_0::t_retdirect_storage_packed => 2235153
//@ run-call: Cud_0::t_param_storage_packed => 2235153
//@ run-call: Cud_0::t_mem_storage_packed => 2235153
//@ run-call: Cud_0::t_local_storage_struct_copy => 27
//@ run-call: Cud_0::t_retdirect_storage_struct_copy => 27
//@ run-call: Cud_0::t_param_storage_struct_copy => 27
//@ run-call: Cud_0::t_mem_storage_struct_copy => 27
//@ run-call: Cud_1::t_local_storage_push => 27
//@ run-call: Cud_1::t_retdirect_storage_push => 27
//@ run-call: Cud_1::t_param_storage_push => 27
//@ run-call: Cud_1::t_mem_storage_push => 27
//@ run-call: Cud_1::t_local_storage_array_elem => 27
//@ run-call: Cud_1::t_retdirect_storage_array_elem => 27
//@ run-call: Cud_1::t_param_storage_array_elem => 27
//@ run-call: Cud_1::t_mem_storage_array_elem => 27
//@ run-call: Cud_1::t_local_mapping_value_key_dirty => 5
//@ run-call: Cud_1::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cud_1::t_param_mapping_value_key_dirty => 5
//@ run-call: Cud_1::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cud_1::t_local_mapping_key_read => 9
//@ run-call: Cud_1::t_retdirect_mapping_key_read => 9
//@ run-call: Cud_1::t_param_mapping_key_read => 9
//@ run-call: Cud_1::t_mem_mapping_key_read => 9
//@ run-call: Cud_1::t_local_mapping_value_raw => 27
//@ run-call: Cud_1::t_retdirect_mapping_value_raw => 27
//@ run-call: Cud_1::t_param_mapping_value_raw => 27
//@ run-call: Cud_1::t_mem_mapping_value_raw => 27

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: memory and storage stores, keys, indexes, slices, allocations.

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
    uint8 sv;
    uint256 gap;
    struct P { uint8 a; uint8 f; uint8 c; }
    P p;
    St_u8 q;
    function _s_retdirect_marray_elem() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_elem(uint8 pv) internal view returns (uint256) { uint8[] memory a = new uint8[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_fixed(uint8 pv) internal view returns (uint256) { uint8[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_literal(uint8 pv) internal view returns (uint256) { uint8[2] memory a = [pv, uint8(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_literal_dyn(uint8 pv) internal view returns (uint256) { uint8[] memory a = new uint8[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mstruct_field(uint8 pv) internal view returns (uint256) { St_u8 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mstruct_literal(uint8 pv) internal view returns (uint256) { St_u8 memory s = St_u8(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mnested(uint8 pv) internal view returns (uint256) { uint8[][] memory a = new uint8[][](1); a[0] = new uint8[](1); a[0][0] = pv; uint8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_scalar(uint8 pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_packed(uint8 pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_struct_copy(uint8 pv) internal  returns (uint256) { St_u8 memory s = St_u8(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[] memory a = new uint8[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { uint8[] memory a = new uint8[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[] memory a = new uint8[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { uint8[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[2] memory a = [rx, uint8(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { uint8[2] memory a = [_s_retdirect_marray_literal(), uint8(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[2] memory a = [ry, uint8(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[] memory a = new uint8[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { uint8[] memory a = new uint8[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[] memory a = new uint8[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } St_u8 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_u8 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; St_u8 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } St_u8 memory s = St_u8(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_u8 memory s = St_u8(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; St_u8 memory s = St_u8(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[][] memory a = new uint8[][](1); a[0] = new uint8[](1); a[0][0] = rx; uint8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { uint8[][] memory a = new uint8[][](1); a[0] = new uint8[](1); a[0][0] = _s_retdirect_mnested(); uint8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[][] memory a = new uint8[][](1); a[0] = new uint8[](1); a[0][0] = ry; uint8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } St_u8 memory s = St_u8(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_u8 memory s = St_u8(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; St_u8 memory s = St_u8(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Cu8_1 {
    uint8[] arr;
    mapping(uint8 => uint256) mp;
    mapping(uint256 => uint8) mv;
    uint256[] sa;
    uint256[40] sf;
    function _s_retdirect_storage_push() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_push(uint8 pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_array_elem(uint8 pv) internal  returns (uint256) { arr.push(uint8(3)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_value_key_dirty(uint8 pv) internal  returns (uint256) { mp[pv] = 5; if (mp[uint8(27)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_key_read(uint8 pv) internal  returns (uint256) { mp[uint8(27)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_value_raw(uint8 pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function _s_retdirect_marray_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_index(uint8 pv) internal view returns (uint256) { uint256[] memory a = new uint256[](40); a[27] = 70; return a[pv]; }
    function _s_retdirect_marray_index_store() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_index_store(uint8 pv) internal view returns (uint256) { uint256[] memory a = new uint256[](40); a[pv] = 71; return a[27]; }
    function _s_retdirect_sarray_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_sarray_index(uint8 pv) internal  returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[pv]; }
    function _s_retdirect_sarray_index_store() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_sarray_index_store(uint8 pv) internal  returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(0); sa[pv] = 72; return sa[27]; }
    function _s_retdirect_sfixed_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_sfixed_index(uint8 pv) internal  returns (uint256) { sf[27] = 73; return sf[pv]; }
    function t_local_storage_push() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } arr.push(uint8(3)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(uint8(3)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; arr.push(uint8(3)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } mp[rx] = 5; if (mp[uint8(27)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[uint8(27)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; mp[ry] = 5; if (mp[uint8(27)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } mp[uint8(27)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[uint8(27)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; mp[uint8(27)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_marray_index() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint256[] memory a = new uint256[](40); a[27] = 70; return a[rx]; }
    function t_retdirect_marray_index() external view returns (uint256) { uint256[] memory a = new uint256[](40); a[27] = 70; return a[_s_retdirect_marray_index()]; }
    function t_param_marray_index() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_index(rx); }
    function t_mem_marray_index() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint256[] memory a = new uint256[](40); a[27] = 70; return a[ry]; }
    function t_local_marray_index_store() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint256[] memory a = new uint256[](40); a[rx] = 71; return a[27]; }
    function t_retdirect_marray_index_store() external view returns (uint256) { uint256[] memory a = new uint256[](40); a[_s_retdirect_marray_index_store()] = 71; return a[27]; }
    function t_param_marray_index_store() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_index_store(rx); }
    function t_mem_marray_index_store() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint256[] memory a = new uint256[](40); a[ry] = 71; return a[27]; }
    function t_local_sarray_index() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[rx]; }
    function t_retdirect_sarray_index() external  returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[_s_retdirect_sarray_index()]; }
    function t_param_sarray_index() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_sarray_index(rx); }
    function t_mem_sarray_index() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[ry]; }
    function t_local_sarray_index_store() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } for (uint256 i; i < 40; ++i) sa.push(0); sa[rx] = 72; return sa[27]; }
    function t_retdirect_sarray_index_store() external  returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(0); sa[_s_retdirect_sarray_index_store()] = 72; return sa[27]; }
    function t_param_sarray_index_store() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_sarray_index_store(rx); }
    function t_mem_sarray_index_store() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; for (uint256 i; i < 40; ++i) sa.push(0); sa[ry] = 72; return sa[27]; }
    function t_local_sfixed_index() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } sf[27] = 73; return sf[rx]; }
    function t_retdirect_sfixed_index() external  returns (uint256) { sf[27] = 73; return sf[_s_retdirect_sfixed_index()]; }
    function t_param_sfixed_index() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_sfixed_index(rx); }
    function t_mem_sfixed_index() external  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; sf[27] = 73; return sf[ry]; }
}
contract Cu8_2 {
    function _s_retdirect_bytes_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_bytes_index(uint8 pv) internal view returns (uint256) { bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[pv])); }
    function _s_retdirect_b32_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_b32_index(uint8 pv) internal view returns (uint256) { bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[pv])); }
    function _s_retdirect_new_array_len() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_new_array_len(uint8 pv) internal view returns (uint256) { uint256[] memory a = new uint256[](pv); return a.length; }
    function _s_retdirect_new_bytes_len() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_new_bytes_len(uint8 pv) internal view returns (uint256) { bytes memory bb = new bytes(pv); return bb.length; }
    function _s_retdirect_cd_array_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_retdirect_cd_bytes_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_retdirect_cd_slice_len() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_retdirect_cd_slice_end() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_retdirect_cd_slice_both() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_retdirect_cd_array_slice() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function t_local_bytes_index() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[rx])); }
    function t_retdirect_bytes_index() external view returns (uint256) { bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[_s_retdirect_bytes_index()])); }
    function t_param_bytes_index() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_bytes_index(rx); }
    function t_mem_bytes_index() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[ry])); }
    function t_local_b32_index() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[rx])); }
    function t_retdirect_b32_index() external view returns (uint256) { bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[_s_retdirect_b32_index()])); }
    function t_param_b32_index() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_b32_index(rx); }
    function t_mem_b32_index() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[ry])); }
    function t_local_new_array_len() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint256[] memory a = new uint256[](rx); return a.length; }
    function t_retdirect_new_array_len() external view returns (uint256) { uint256[] memory a = new uint256[](_s_retdirect_new_array_len()); return a.length; }
    function t_param_new_array_len() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_new_array_len(rx); }
    function t_mem_new_array_len() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint256[] memory a = new uint256[](ry); return a.length; }
    function t_local_new_bytes_len() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bytes memory bb = new bytes(rx); return bb.length; }
    function t_retdirect_new_bytes_len() external view returns (uint256) { bytes memory bb = new bytes(_s_retdirect_new_bytes_len()); return bb.length; }
    function t_param_new_bytes_len() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_new_bytes_len(rx); }
    function t_mem_new_bytes_len() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; bytes memory bb = new bytes(ry); return bb.length; }
    function t_local_cd_array_index(uint256[] calldata a) external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return a[rx]; }
    function t_retdirect_cd_array_index(uint256[] calldata a) external view returns (uint256) { return a[_s_retdirect_cd_array_index()]; }
    function t_mem_cd_array_index(uint256[] calldata a) external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return a[ry]; }
    function t_local_cd_bytes_index(bytes calldata d) external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(d[rx])); }
    function t_retdirect_cd_bytes_index(bytes calldata d) external view returns (uint256) { return uint256(uint8(d[_s_retdirect_cd_bytes_index()])); }
    function t_mem_cd_bytes_index(bytes calldata d) external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(d[ry])); }
    function t_local_cd_slice_len(bytes calldata d) external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return bytes(d[rx:]).length; }
    function t_retdirect_cd_slice_len(bytes calldata d) external view returns (uint256) { return bytes(d[_s_retdirect_cd_slice_len():]).length; }
    function t_mem_cd_slice_len(bytes calldata d) external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return bytes(d[ry:]).length; }
    function t_local_cd_slice_end(bytes calldata d) external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return bytes(d[:rx]).length; }
    function t_retdirect_cd_slice_end(bytes calldata d) external view returns (uint256) { return bytes(d[:_s_retdirect_cd_slice_end()]).length; }
    function t_mem_cd_slice_end(bytes calldata d) external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return bytes(d[:ry]).length; }
    function t_local_cd_slice_both(bytes calldata d) external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return bytes(d[rx:rx]).length; }
    function t_retdirect_cd_slice_both(bytes calldata d) external view returns (uint256) { return bytes(d[_s_retdirect_cd_slice_both():_s_retdirect_cd_slice_both()]).length; }
    function t_mem_cd_slice_both(bytes calldata d) external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return bytes(d[ry:ry]).length; }
    function t_local_cd_array_slice(uint256[] calldata a) external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return a[rx:][0]; }
    function t_retdirect_cd_array_slice(uint256[] calldata a) external view returns (uint256) { return a[_s_retdirect_cd_array_slice():][0]; }
    function t_mem_cd_array_slice(uint256[] calldata a) external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return a[ry:][0]; }
}
contract Cu64_0 {
    uint64 sv;
    uint256 gap;
    struct P { uint8 a; uint64 f; uint8 c; }
    P p;
    St_u64 q;
    function _s_retdirect_marray_elem() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_elem(uint64 pv) internal view returns (uint256) { uint64[] memory a = new uint64[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_fixed(uint64 pv) internal view returns (uint256) { uint64[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_literal(uint64 pv) internal view returns (uint256) { uint64[2] memory a = [pv, uint64(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_literal_dyn(uint64 pv) internal view returns (uint256) { uint64[] memory a = new uint64[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mstruct_field(uint64 pv) internal view returns (uint256) { St_u64 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mstruct_literal(uint64 pv) internal view returns (uint256) { St_u64 memory s = St_u64(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mnested(uint64 pv) internal view returns (uint256) { uint64[][] memory a = new uint64[][](1); a[0] = new uint64[](1); a[0][0] = pv; uint64[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_storage_scalar(uint64 pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_storage_packed(uint64 pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_storage_struct_copy(uint64 pv) internal  returns (uint256) { St_u64 memory s = St_u64(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[] memory a = new uint64[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { uint64[] memory a = new uint64[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[] memory a = new uint64[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { uint64[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[2] memory a = [rx, uint64(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { uint64[2] memory a = [_s_retdirect_marray_literal(), uint64(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[2] memory a = [ry, uint64(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[] memory a = new uint64[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { uint64[] memory a = new uint64[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[] memory a = new uint64[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } St_u64 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_u64 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; St_u64 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } St_u64 memory s = St_u64(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_u64 memory s = St_u64(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; St_u64 memory s = St_u64(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[][] memory a = new uint64[][](1); a[0] = new uint64[](1); a[0][0] = rx; uint64[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { uint64[][] memory a = new uint64[][](1); a[0] = new uint64[](1); a[0][0] = _s_retdirect_mnested(); uint64[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[][] memory a = new uint64[][](1); a[0] = new uint64[](1); a[0][0] = ry; uint64[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } St_u64 memory s = St_u64(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_u64 memory s = St_u64(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; St_u64 memory s = St_u64(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Cu64_1 {
    uint64[] arr;
    mapping(uint64 => uint256) mp;
    mapping(uint256 => uint64) mv;
    uint256[] sa;
    uint256[40] sf;
    function _s_retdirect_storage_push() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_storage_push(uint64 pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_storage_array_elem(uint64 pv) internal  returns (uint256) { arr.push(uint64(3)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mapping_value_key_dirty(uint64 pv) internal  returns (uint256) { mp[pv] = 5; if (mp[uint64(27)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mapping_key_read(uint64 pv) internal  returns (uint256) { mp[uint64(27)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mapping_value_raw(uint64 pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function _s_retdirect_marray_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_index(uint64 pv) internal view returns (uint256) { uint256[] memory a = new uint256[](40); a[27] = 70; return a[pv]; }
    function _s_retdirect_marray_index_store() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_index_store(uint64 pv) internal view returns (uint256) { uint256[] memory a = new uint256[](40); a[pv] = 71; return a[27]; }
    function _s_retdirect_sarray_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_sarray_index(uint64 pv) internal  returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[pv]; }
    function _s_retdirect_sarray_index_store() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_sarray_index_store(uint64 pv) internal  returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(0); sa[pv] = 72; return sa[27]; }
    function _s_retdirect_sfixed_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_sfixed_index(uint64 pv) internal  returns (uint256) { sf[27] = 73; return sf[pv]; }
    function t_local_storage_push() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } arr.push(uint64(3)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(uint64(3)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; arr.push(uint64(3)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } mp[rx] = 5; if (mp[uint64(27)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[uint64(27)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; mp[ry] = 5; if (mp[uint64(27)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } mp[uint64(27)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[uint64(27)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; mp[uint64(27)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_marray_index() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint256[] memory a = new uint256[](40); a[27] = 70; return a[rx]; }
    function t_retdirect_marray_index() external view returns (uint256) { uint256[] memory a = new uint256[](40); a[27] = 70; return a[_s_retdirect_marray_index()]; }
    function t_param_marray_index() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_index(rx); }
    function t_mem_marray_index() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint256[] memory a = new uint256[](40); a[27] = 70; return a[ry]; }
    function t_local_marray_index_store() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint256[] memory a = new uint256[](40); a[rx] = 71; return a[27]; }
    function t_retdirect_marray_index_store() external view returns (uint256) { uint256[] memory a = new uint256[](40); a[_s_retdirect_marray_index_store()] = 71; return a[27]; }
    function t_param_marray_index_store() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_index_store(rx); }
    function t_mem_marray_index_store() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint256[] memory a = new uint256[](40); a[ry] = 71; return a[27]; }
    function t_local_sarray_index() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[rx]; }
    function t_retdirect_sarray_index() external  returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[_s_retdirect_sarray_index()]; }
    function t_param_sarray_index() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_sarray_index(rx); }
    function t_mem_sarray_index() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[ry]; }
    function t_local_sarray_index_store() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } for (uint256 i; i < 40; ++i) sa.push(0); sa[rx] = 72; return sa[27]; }
    function t_retdirect_sarray_index_store() external  returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(0); sa[_s_retdirect_sarray_index_store()] = 72; return sa[27]; }
    function t_param_sarray_index_store() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_sarray_index_store(rx); }
    function t_mem_sarray_index_store() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; for (uint256 i; i < 40; ++i) sa.push(0); sa[ry] = 72; return sa[27]; }
    function t_local_sfixed_index() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } sf[27] = 73; return sf[rx]; }
    function t_retdirect_sfixed_index() external  returns (uint256) { sf[27] = 73; return sf[_s_retdirect_sfixed_index()]; }
    function t_param_sfixed_index() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_sfixed_index(rx); }
    function t_mem_sfixed_index() external  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; sf[27] = 73; return sf[ry]; }
}
contract Cu64_2 {
    function _s_retdirect_bytes_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_bytes_index(uint64 pv) internal view returns (uint256) { bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[pv])); }
    function _s_retdirect_b32_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_b32_index(uint64 pv) internal view returns (uint256) { bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[pv])); }
    function _s_retdirect_new_array_len() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_new_array_len(uint64 pv) internal view returns (uint256) { uint256[] memory a = new uint256[](pv); return a.length; }
    function _s_retdirect_new_bytes_len() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_new_bytes_len(uint64 pv) internal view returns (uint256) { bytes memory bb = new bytes(pv); return bb.length; }
    function _s_retdirect_cd_array_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_retdirect_cd_bytes_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_retdirect_cd_slice_len() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_retdirect_cd_slice_end() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_retdirect_cd_slice_both() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_retdirect_cd_array_slice() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function t_local_bytes_index() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[rx])); }
    function t_retdirect_bytes_index() external view returns (uint256) { bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[_s_retdirect_bytes_index()])); }
    function t_param_bytes_index() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_bytes_index(rx); }
    function t_mem_bytes_index() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[ry])); }
    function t_local_b32_index() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[rx])); }
    function t_retdirect_b32_index() external view returns (uint256) { bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[_s_retdirect_b32_index()])); }
    function t_param_b32_index() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_b32_index(rx); }
    function t_mem_b32_index() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[ry])); }
    function t_local_new_array_len() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint256[] memory a = new uint256[](rx); return a.length; }
    function t_retdirect_new_array_len() external view returns (uint256) { uint256[] memory a = new uint256[](_s_retdirect_new_array_len()); return a.length; }
    function t_param_new_array_len() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_new_array_len(rx); }
    function t_mem_new_array_len() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint256[] memory a = new uint256[](ry); return a.length; }
    function t_local_new_bytes_len() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bytes memory bb = new bytes(rx); return bb.length; }
    function t_retdirect_new_bytes_len() external view returns (uint256) { bytes memory bb = new bytes(_s_retdirect_new_bytes_len()); return bb.length; }
    function t_param_new_bytes_len() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_new_bytes_len(rx); }
    function t_mem_new_bytes_len() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; bytes memory bb = new bytes(ry); return bb.length; }
    function t_local_cd_array_index(uint256[] calldata a) external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return a[rx]; }
    function t_retdirect_cd_array_index(uint256[] calldata a) external view returns (uint256) { return a[_s_retdirect_cd_array_index()]; }
    function t_mem_cd_array_index(uint256[] calldata a) external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return a[ry]; }
    function t_local_cd_bytes_index(bytes calldata d) external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint8(d[rx])); }
    function t_retdirect_cd_bytes_index(bytes calldata d) external view returns (uint256) { return uint256(uint8(d[_s_retdirect_cd_bytes_index()])); }
    function t_mem_cd_bytes_index(bytes calldata d) external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint8(d[ry])); }
    function t_local_cd_slice_len(bytes calldata d) external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return bytes(d[rx:]).length; }
    function t_retdirect_cd_slice_len(bytes calldata d) external view returns (uint256) { return bytes(d[_s_retdirect_cd_slice_len():]).length; }
    function t_mem_cd_slice_len(bytes calldata d) external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return bytes(d[ry:]).length; }
    function t_local_cd_slice_end(bytes calldata d) external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return bytes(d[:rx]).length; }
    function t_retdirect_cd_slice_end(bytes calldata d) external view returns (uint256) { return bytes(d[:_s_retdirect_cd_slice_end()]).length; }
    function t_mem_cd_slice_end(bytes calldata d) external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return bytes(d[:ry]).length; }
    function t_local_cd_slice_both(bytes calldata d) external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return bytes(d[rx:rx]).length; }
    function t_retdirect_cd_slice_both(bytes calldata d) external view returns (uint256) { return bytes(d[_s_retdirect_cd_slice_both():_s_retdirect_cd_slice_both()]).length; }
    function t_mem_cd_slice_both(bytes calldata d) external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return bytes(d[ry:ry]).length; }
    function t_local_cd_array_slice(uint256[] calldata a) external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return a[rx:][0]; }
    function t_retdirect_cd_array_slice(uint256[] calldata a) external view returns (uint256) { return a[_s_retdirect_cd_array_slice():][0]; }
    function t_mem_cd_array_slice(uint256[] calldata a) external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return a[ry:][0]; }
}
contract Ci8_0 {
    int8 sv;
    uint256 gap;
    struct P { uint8 a; int8 f; uint8 c; }
    P p;
    St_i8 q;
    function _s_retdirect_marray_elem() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_marray_elem(int8 pv) internal view returns (uint256) { int8[] memory a = new int8[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_marray_fixed(int8 pv) internal view returns (uint256) { int8[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_marray_literal(int8 pv) internal view returns (uint256) { int8[2] memory a = [pv, int8(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_marray_literal_dyn(int8 pv) internal view returns (uint256) { int8[] memory a = new int8[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mstruct_field(int8 pv) internal view returns (uint256) { St_i8 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mstruct_literal(int8 pv) internal view returns (uint256) { St_i8 memory s = St_i8(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mnested(int8 pv) internal view returns (uint256) { int8[][] memory a = new int8[][](1); a[0] = new int8[](1); a[0][0] = pv; int8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_storage_scalar(int8 pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_storage_packed(int8 pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_storage_struct_copy(int8 pv) internal  returns (uint256) { St_i8 memory s = St_i8(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[] memory a = new int8[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { int8[] memory a = new int8[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[] memory a = new int8[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { int8[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[2] memory a = [rx, int8(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { int8[2] memory a = [_s_retdirect_marray_literal(), int8(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[2] memory a = [ry, int8(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[] memory a = new int8[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { int8[] memory a = new int8[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[] memory a = new int8[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } St_i8 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_i8 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; St_i8 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } St_i8 memory s = St_i8(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_i8 memory s = St_i8(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; St_i8 memory s = St_i8(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[][] memory a = new int8[][](1); a[0] = new int8[](1); a[0][0] = rx; int8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { int8[][] memory a = new int8[][](1); a[0] = new int8[](1); a[0][0] = _s_retdirect_mnested(); int8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[][] memory a = new int8[][](1); a[0] = new int8[](1); a[0][0] = ry; int8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } St_i8 memory s = St_i8(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_i8 memory s = St_i8(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; St_i8 memory s = St_i8(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Ci8_1 {
    int8[] arr;
    mapping(int8 => uint256) mp;
    mapping(uint256 => int8) mv;
    function _s_retdirect_storage_push() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_storage_push(int8 pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_storage_array_elem(int8 pv) internal  returns (uint256) { arr.push(int8(-2)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mapping_value_key_dirty(int8 pv) internal  returns (uint256) { mp[pv] = 5; if (mp[int8(-3)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mapping_key_read(int8 pv) internal  returns (uint256) { mp[int8(-3)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mapping_value_raw(int8 pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_storage_push() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } arr.push(int8(-2)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(int8(-2)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; arr.push(int8(-2)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } mp[rx] = 5; if (mp[int8(-3)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[int8(-3)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; mp[ry] = 5; if (mp[int8(-3)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } mp[int8(-3)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[int8(-3)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; mp[int8(-3)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Ci24_0 {
    int24 sv;
    uint256 gap;
    struct P { uint8 a; int24 f; uint8 c; }
    P p;
    St_i24 q;
    function _s_retdirect_marray_elem() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_marray_elem(int24 pv) internal view returns (uint256) { int24[] memory a = new int24[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_marray_fixed(int24 pv) internal view returns (uint256) { int24[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_marray_literal(int24 pv) internal view returns (uint256) { int24[2] memory a = [pv, int24(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_marray_literal_dyn(int24 pv) internal view returns (uint256) { int24[] memory a = new int24[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mstruct_field(int24 pv) internal view returns (uint256) { St_i24 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mstruct_literal(int24 pv) internal view returns (uint256) { St_i24 memory s = St_i24(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mnested(int24 pv) internal view returns (uint256) { int24[][] memory a = new int24[][](1); a[0] = new int24[](1); a[0][0] = pv; int24[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_storage_scalar(int24 pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_storage_packed(int24 pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_storage_struct_copy(int24 pv) internal  returns (uint256) { St_i24 memory s = St_i24(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[] memory a = new int24[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { int24[] memory a = new int24[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[] memory a = new int24[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { int24[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[2] memory a = [rx, int24(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { int24[2] memory a = [_s_retdirect_marray_literal(), int24(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[2] memory a = [ry, int24(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[] memory a = new int24[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { int24[] memory a = new int24[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[] memory a = new int24[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } St_i24 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_i24 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; St_i24 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } St_i24 memory s = St_i24(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_i24 memory s = St_i24(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; St_i24 memory s = St_i24(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[][] memory a = new int24[][](1); a[0] = new int24[](1); a[0][0] = rx; int24[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { int24[][] memory a = new int24[][](1); a[0] = new int24[](1); a[0][0] = _s_retdirect_mnested(); int24[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[][] memory a = new int24[][](1); a[0] = new int24[](1); a[0][0] = ry; int24[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } St_i24 memory s = St_i24(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_i24 memory s = St_i24(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; St_i24 memory s = St_i24(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Ci24_1 {
    int24[] arr;
    mapping(int24 => uint256) mp;
    mapping(uint256 => int24) mv;
    function _s_retdirect_storage_push() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_storage_push(int24 pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_storage_array_elem(int24 pv) internal  returns (uint256) { arr.push(int24(-2)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mapping_value_key_dirty(int24 pv) internal  returns (uint256) { mp[pv] = 5; if (mp[int24(-3)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mapping_key_read(int24 pv) internal  returns (uint256) { mp[int24(-3)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mapping_value_raw(int24 pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_storage_push() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } arr.push(int24(-2)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(int24(-2)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; arr.push(int24(-2)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } mp[rx] = 5; if (mp[int24(-3)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[int24(-3)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; mp[ry] = 5; if (mp[int24(-3)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } mp[int24(-3)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[int24(-3)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; mp[int24(-3)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Caddr_0 {
    address sv;
    uint256 gap;
    struct P { uint8 a; address f; uint8 c; }
    P p;
    St_addr q;
    function _s_retdirect_marray_elem() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_elem(address pv) internal view returns (uint256) { address[] memory a = new address[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_fixed(address pv) internal view returns (uint256) { address[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal(address pv) internal view returns (uint256) { address[2] memory a = [pv, address(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal_dyn(address pv) internal view returns (uint256) { address[] memory a = new address[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_field(address pv) internal view returns (uint256) { St_addr memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_literal(address pv) internal view returns (uint256) { St_addr memory s = St_addr(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mnested(address pv) internal view returns (uint256) { address[][] memory a = new address[][](1); a[0] = new address[](1); a[0][0] = pv; address[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_scalar(address pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_packed(address pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_struct_copy(address pv) internal  returns (uint256) { St_addr memory s = St_addr(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[] memory a = new address[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { address[] memory a = new address[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[] memory a = new address[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { address[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[2] memory a = [rx, address(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { address[2] memory a = [_s_retdirect_marray_literal(), address(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[2] memory a = [ry, address(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[] memory a = new address[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { address[] memory a = new address[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[] memory a = new address[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addr memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_addr memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; St_addr memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addr memory s = St_addr(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_addr memory s = St_addr(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; St_addr memory s = St_addr(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address[][] memory a = new address[][](1); a[0] = new address[](1); a[0][0] = rx; address[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { address[][] memory a = new address[][](1); a[0] = new address[](1); a[0][0] = _s_retdirect_mnested(); address[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address[][] memory a = new address[][](1); a[0] = new address[](1); a[0][0] = ry; address[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addr memory s = St_addr(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_addr memory s = St_addr(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; St_addr memory s = St_addr(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Caddr_1 {
    address[] arr;
    mapping(address => uint256) mp;
    mapping(uint256 => address) mv;
    function _s_retdirect_storage_push() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_push(address pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_array_elem(address pv) internal  returns (uint256) { arr.push(address(0x1111111111111111111111111111111111111111)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_key_dirty(address pv) internal  returns (uint256) { mp[pv] = 5; if (mp[address(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_key_read(address pv) internal  returns (uint256) { mp[address(0x1234567890123456789012345678901234567890)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_raw(address pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_storage_push() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(address(0x1111111111111111111111111111111111111111)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(address(0x1111111111111111111111111111111111111111)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; arr.push(address(0x1111111111111111111111111111111111111111)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[rx] = 5; if (mp[address(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[address(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; mp[ry] = 5; if (mp[address(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[address(0x1234567890123456789012345678901234567890)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[address(0x1234567890123456789012345678901234567890)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; mp[address(0x1234567890123456789012345678901234567890)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Caddrp_0 {
    address payable sv;
    uint256 gap;
    struct P { uint8 a; address payable f; uint8 c; }
    P p;
    St_addrp q;
    function _s_retdirect_marray_elem() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_elem(address payable pv) internal view returns (uint256) { address payable[] memory a = new address payable[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_fixed(address payable pv) internal view returns (uint256) { address payable[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal(address payable pv) internal view returns (uint256) { address payable[2] memory a = [pv, payable(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal_dyn(address payable pv) internal view returns (uint256) { address payable[] memory a = new address payable[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_field(address payable pv) internal view returns (uint256) { St_addrp memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_literal(address payable pv) internal view returns (uint256) { St_addrp memory s = St_addrp(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mnested(address payable pv) internal view returns (uint256) { address payable[][] memory a = new address payable[][](1); a[0] = new address payable[](1); a[0][0] = pv; address payable[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_scalar(address payable pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_packed(address payable pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_struct_copy(address payable pv) internal  returns (uint256) { St_addrp memory s = St_addrp(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[] memory a = new address payable[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { address payable[] memory a = new address payable[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[] memory a = new address payable[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { address payable[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[2] memory a = [rx, payable(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { address payable[2] memory a = [_s_retdirect_marray_literal(), payable(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[2] memory a = [ry, payable(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[] memory a = new address payable[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { address payable[] memory a = new address payable[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[] memory a = new address payable[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addrp memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_addrp memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; St_addrp memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addrp memory s = St_addrp(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_addrp memory s = St_addrp(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; St_addrp memory s = St_addrp(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable[][] memory a = new address payable[][](1); a[0] = new address payable[](1); a[0][0] = rx; address payable[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { address payable[][] memory a = new address payable[][](1); a[0] = new address payable[](1); a[0][0] = _s_retdirect_mnested(); address payable[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable[][] memory a = new address payable[][](1); a[0] = new address payable[](1); a[0][0] = ry; address payable[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addrp memory s = St_addrp(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_addrp memory s = St_addrp(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; St_addrp memory s = St_addrp(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Caddrp_1 {
    address payable[] arr;
    mapping(address => uint256) mp;
    mapping(uint256 => address payable) mv;
    function _s_retdirect_storage_push() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_push(address payable pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_array_elem(address payable pv) internal  returns (uint256) { arr.push(payable(address(0x1111111111111111111111111111111111111111))); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_key_dirty(address payable pv) internal  returns (uint256) { mp[pv] = 5; if (mp[payable(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_key_read(address payable pv) internal  returns (uint256) { mp[payable(address(0x1234567890123456789012345678901234567890))] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_raw(address payable pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_storage_push() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(payable(address(0x1111111111111111111111111111111111111111))); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(payable(address(0x1111111111111111111111111111111111111111))); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; arr.push(payable(address(0x1111111111111111111111111111111111111111))); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[rx] = 5; if (mp[payable(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[payable(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; mp[ry] = 5; if (mp[payable(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[payable(address(0x1234567890123456789012345678901234567890))] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[payable(address(0x1234567890123456789012345678901234567890))] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; mp[payable(address(0x1234567890123456789012345678901234567890))] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cctr_0 {
    IC sv;
    uint256 gap;
    struct P { uint8 a; IC f; uint8 c; }
    P p;
    St_ctr q;
    function _s_retdirect_marray_elem() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_elem(IC pv) internal view returns (uint256) { IC[] memory a = new IC[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_fixed(IC pv) internal view returns (uint256) { IC[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal(IC pv) internal view returns (uint256) { IC[2] memory a = [pv, IC(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_marray_literal_dyn(IC pv) internal view returns (uint256) { IC[] memory a = new IC[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_field(IC pv) internal view returns (uint256) { St_ctr memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_literal(IC pv) internal view returns (uint256) { St_ctr memory s = St_ctr(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mnested(IC pv) internal view returns (uint256) { IC[][] memory a = new IC[][](1); a[0] = new IC[](1); a[0][0] = pv; IC[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_scalar(IC pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_packed(IC pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_struct_copy(IC pv) internal  returns (uint256) { St_ctr memory s = St_ctr(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[] memory a = new IC[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { IC[] memory a = new IC[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[] memory a = new IC[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { IC[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[2] memory a = [rx, IC(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { IC[2] memory a = [_s_retdirect_marray_literal(), IC(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[2] memory a = [ry, IC(address(0x1111111111111111111111111111111111111111))]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[] memory a = new IC[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { IC[] memory a = new IC[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[] memory a = new IC[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_ctr memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_ctr memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; St_ctr memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_ctr memory s = St_ctr(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_ctr memory s = St_ctr(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; St_ctr memory s = St_ctr(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC[][] memory a = new IC[][](1); a[0] = new IC[](1); a[0][0] = rx; IC[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { IC[][] memory a = new IC[][](1); a[0] = new IC[](1); a[0][0] = _s_retdirect_mnested(); IC[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC[][] memory a = new IC[][](1); a[0] = new IC[](1); a[0][0] = ry; IC[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_ctr memory s = St_ctr(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_ctr memory s = St_ctr(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; St_ctr memory s = St_ctr(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Cctr_1 {
    IC[] arr;
    mapping(IC => uint256) mp;
    mapping(uint256 => IC) mv;
    function _s_retdirect_storage_push() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_push(IC pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_array_elem(IC pv) internal  returns (uint256) { arr.push(IC(address(0x1111111111111111111111111111111111111111))); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_key_dirty(IC pv) internal  returns (uint256) { mp[pv] = 5; if (mp[IC(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_key_read(IC pv) internal  returns (uint256) { mp[IC(address(0x1234567890123456789012345678901234567890))] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_raw(IC pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_storage_push() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(IC(address(0x1111111111111111111111111111111111111111))); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(IC(address(0x1111111111111111111111111111111111111111))); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; arr.push(IC(address(0x1111111111111111111111111111111111111111))); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[rx] = 5; if (mp[IC(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[IC(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; mp[ry] = 5; if (mp[IC(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[IC(address(0x1234567890123456789012345678901234567890))] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[IC(address(0x1234567890123456789012345678901234567890))] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; mp[IC(address(0x1234567890123456789012345678901234567890))] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cbool_0 {
    bool sv;
    uint256 gap;
    struct P { uint8 a; bool f; uint8 c; }
    P p;
    St_bool q;
    function _s_retdirect_marray_elem() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_marray_elem(bool pv) internal view returns (uint256) { bool[] memory a = new bool[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_marray_fixed(bool pv) internal view returns (uint256) { bool[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_marray_literal(bool pv) internal view returns (uint256) { bool[2] memory a = [pv, false]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_marray_literal_dyn(bool pv) internal view returns (uint256) { bool[] memory a = new bool[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mstruct_field(bool pv) internal view returns (uint256) { St_bool memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mstruct_literal(bool pv) internal view returns (uint256) { St_bool memory s = St_bool(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mnested(bool pv) internal view returns (uint256) { bool[][] memory a = new bool[][](1); a[0] = new bool[](1); a[0][0] = pv; bool[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_storage_scalar(bool pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_storage_packed(bool pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_storage_struct_copy(bool pv) internal  returns (uint256) { St_bool memory s = St_bool(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { bool rx; assembly { rx := 0x100 } bool[] memory a = new bool[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { bool[] memory a = new bool[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[] memory a = new bool[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { bool rx; assembly { rx := 0x100 } bool[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { bool[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { bool rx; assembly { rx := 0x100 } bool[2] memory a = [rx, false]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { bool[2] memory a = [_s_retdirect_marray_literal(), false]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[2] memory a = [ry, false]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { bool rx; assembly { rx := 0x100 } bool[] memory a = new bool[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { bool[] memory a = new bool[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[] memory a = new bool[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { bool rx; assembly { rx := 0x100 } St_bool memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_bool memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; St_bool memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { bool rx; assembly { rx := 0x100 } St_bool memory s = St_bool(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_bool memory s = St_bool(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; St_bool memory s = St_bool(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { bool rx; assembly { rx := 0x100 } bool[][] memory a = new bool[][](1); a[0] = new bool[](1); a[0][0] = rx; bool[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { bool[][] memory a = new bool[][](1); a[0] = new bool[](1); a[0][0] = _s_retdirect_mnested(); bool[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[][] memory a = new bool[][](1); a[0] = new bool[](1); a[0][0] = ry; bool[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { bool rx; assembly { rx := 0x100 } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { bool rx; assembly { rx := 0x100 } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { bool rx; assembly { rx := 0x100 } St_bool memory s = St_bool(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_bool memory s = St_bool(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; St_bool memory s = St_bool(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Cbool_1 {
    bool[] arr;
    mapping(bool => uint256) mp;
    mapping(uint256 => bool) mv;
    function _s_retdirect_storage_push() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_storage_push(bool pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_storage_array_elem(bool pv) internal  returns (uint256) { arr.push(false); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mapping_value_key_dirty(bool pv) internal  returns (uint256) { mp[pv] = 5; if (mp[true] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mapping_key_read(bool pv) internal  returns (uint256) { mp[true] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mapping_value_raw(bool pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_storage_push() external  returns (uint256) { bool rx; assembly { rx := 0x100 } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { bool rx; assembly { rx := 0x100 } arr.push(false); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(false); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; arr.push(false); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { bool rx; assembly { rx := 0x100 } mp[rx] = 5; if (mp[true] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[true] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; mp[ry] = 5; if (mp[true] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { bool rx; assembly { rx := 0x100 } mp[true] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[true] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; mp[true] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { bool rx; assembly { rx := 0x100 } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cb1_0 {
    bytes1 sv;
    uint256 gap;
    struct P { uint8 a; bytes1 f; uint8 c; }
    P p;
    St_b1 q;
    function _s_retdirect_marray_elem() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_marray_elem(bytes1 pv) internal view returns (uint256) { bytes1[] memory a = new bytes1[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_marray_fixed(bytes1 pv) internal view returns (uint256) { bytes1[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_marray_literal(bytes1 pv) internal view returns (uint256) { bytes1[2] memory a = [pv, bytes1(0xcd)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_marray_literal_dyn(bytes1 pv) internal view returns (uint256) { bytes1[] memory a = new bytes1[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mstruct_field(bytes1 pv) internal view returns (uint256) { St_b1 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mstruct_literal(bytes1 pv) internal view returns (uint256) { St_b1 memory s = St_b1(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mnested(bytes1 pv) internal view returns (uint256) { bytes1[][] memory a = new bytes1[][](1); a[0] = new bytes1[](1); a[0][0] = pv; bytes1[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_scalar(bytes1 pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_packed(bytes1 pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_struct_copy(bytes1 pv) internal  returns (uint256) { St_b1 memory s = St_b1(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[] memory a = new bytes1[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { bytes1[] memory a = new bytes1[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[] memory a = new bytes1[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { bytes1[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[2] memory a = [rx, bytes1(0xcd)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { bytes1[2] memory a = [_s_retdirect_marray_literal(), bytes1(0xcd)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[2] memory a = [ry, bytes1(0xcd)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[] memory a = new bytes1[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { bytes1[] memory a = new bytes1[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[] memory a = new bytes1[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } St_b1 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_b1 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; St_b1 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } St_b1 memory s = St_b1(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_b1 memory s = St_b1(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; St_b1 memory s = St_b1(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[][] memory a = new bytes1[][](1); a[0] = new bytes1[](1); a[0][0] = rx; bytes1[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { bytes1[][] memory a = new bytes1[][](1); a[0] = new bytes1[](1); a[0][0] = _s_retdirect_mnested(); bytes1[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[][] memory a = new bytes1[][](1); a[0] = new bytes1[](1); a[0][0] = ry; bytes1[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } St_b1 memory s = St_b1(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_b1 memory s = St_b1(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; St_b1 memory s = St_b1(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Cb1_1 {
    bytes1[] arr;
    mapping(bytes1 => uint256) mp;
    mapping(uint256 => bytes1) mv;
    bytes bs;
    bytes1 sb;
    uint256 gap;
    function _s_retdirect_storage_push() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_push(bytes1 pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_array_elem(bytes1 pv) internal  returns (uint256) { arr.push(bytes1(0xcd)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mapping_value_key_dirty(bytes1 pv) internal  returns (uint256) { mp[pv] = 5; if (mp[bytes1(0xab)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mapping_key_read(bytes1 pv) internal  returns (uint256) { mp[bytes1(0xab)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mapping_value_raw(bytes1 pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function _s_retdirect_storage_bytes_push() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_bytes_push(bytes1 pv) internal  returns (uint256) { bs.push(pv); uint256 r; assembly { r := sload(bs.slot) } return r; }
    function _s_retdirect_bytes_to_storage_bytesN() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_bytes_to_storage_bytesN(bytes1 pv) internal  returns (uint256) { sb = pv; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_local_storage_push() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } arr.push(bytes1(0xcd)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(bytes1(0xcd)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; arr.push(bytes1(0xcd)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } mp[rx] = 5; if (mp[bytes1(0xab)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[bytes1(0xab)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; mp[ry] = 5; if (mp[bytes1(0xab)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } mp[bytes1(0xab)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[bytes1(0xab)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; mp[bytes1(0xab)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_storage_bytes_push() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bs.push(rx); uint256 r; assembly { r := sload(bs.slot) } return r; }
    function t_retdirect_storage_bytes_push() external  returns (uint256) { bs.push(_s_retdirect_storage_bytes_push()); uint256 r; assembly { r := sload(bs.slot) } return r; }
    function t_param_storage_bytes_push() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_bytes_push(rx); }
    function t_mem_storage_bytes_push() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bs.push(ry); uint256 r; assembly { r := sload(bs.slot) } return r; }
    function t_local_bytes_to_storage_bytesN() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } sb = rx; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_retdirect_bytes_to_storage_bytesN() external  returns (uint256) { sb = _s_retdirect_bytes_to_storage_bytesN(); uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_param_bytes_to_storage_bytesN() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_bytes_to_storage_bytesN(rx); }
    function t_mem_bytes_to_storage_bytesN() external  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; sb = ry; uint256 r; assembly { r := sload(sb.slot) } return r; }
}
contract Cb4_0 {
    bytes4 sv;
    uint256 gap;
    struct P { uint8 a; bytes4 f; uint8 c; }
    P p;
    St_b4 q;
    function _s_retdirect_marray_elem() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_marray_elem(bytes4 pv) internal view returns (uint256) { bytes4[] memory a = new bytes4[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_marray_fixed(bytes4 pv) internal view returns (uint256) { bytes4[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_marray_literal(bytes4 pv) internal view returns (uint256) { bytes4[2] memory a = [pv, bytes4(0x11223344)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_marray_literal_dyn(bytes4 pv) internal view returns (uint256) { bytes4[] memory a = new bytes4[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mstruct_field(bytes4 pv) internal view returns (uint256) { St_b4 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mstruct_literal(bytes4 pv) internal view returns (uint256) { St_b4 memory s = St_b4(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mnested(bytes4 pv) internal view returns (uint256) { bytes4[][] memory a = new bytes4[][](1); a[0] = new bytes4[](1); a[0][0] = pv; bytes4[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_storage_scalar(bytes4 pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_storage_packed(bytes4 pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_storage_struct_copy(bytes4 pv) internal  returns (uint256) { St_b4 memory s = St_b4(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[] memory a = new bytes4[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { bytes4[] memory a = new bytes4[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[] memory a = new bytes4[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { bytes4[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[2] memory a = [rx, bytes4(0x11223344)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { bytes4[2] memory a = [_s_retdirect_marray_literal(), bytes4(0x11223344)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[2] memory a = [ry, bytes4(0x11223344)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[] memory a = new bytes4[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { bytes4[] memory a = new bytes4[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[] memory a = new bytes4[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } St_b4 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_b4 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; St_b4 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } St_b4 memory s = St_b4(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_b4 memory s = St_b4(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; St_b4 memory s = St_b4(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[][] memory a = new bytes4[][](1); a[0] = new bytes4[](1); a[0][0] = rx; bytes4[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { bytes4[][] memory a = new bytes4[][](1); a[0] = new bytes4[](1); a[0][0] = _s_retdirect_mnested(); bytes4[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[][] memory a = new bytes4[][](1); a[0] = new bytes4[](1); a[0][0] = ry; bytes4[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } St_b4 memory s = St_b4(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_b4 memory s = St_b4(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; St_b4 memory s = St_b4(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Cb4_1 {
    bytes4[] arr;
    mapping(bytes4 => uint256) mp;
    mapping(uint256 => bytes4) mv;
    bytes4 sb;
    uint256 gap;
    function _s_retdirect_storage_push() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_storage_push(bytes4 pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_storage_array_elem(bytes4 pv) internal  returns (uint256) { arr.push(bytes4(0x11223344)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mapping_value_key_dirty(bytes4 pv) internal  returns (uint256) { mp[pv] = 5; if (mp[bytes4(0xdeadbeef)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mapping_key_read(bytes4 pv) internal  returns (uint256) { mp[bytes4(0xdeadbeef)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mapping_value_raw(bytes4 pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function _s_retdirect_bytes_to_storage_bytesN() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_bytes_to_storage_bytesN(bytes4 pv) internal  returns (uint256) { sb = pv; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_local_storage_push() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } arr.push(bytes4(0x11223344)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(bytes4(0x11223344)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; arr.push(bytes4(0x11223344)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } mp[rx] = 5; if (mp[bytes4(0xdeadbeef)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[bytes4(0xdeadbeef)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; mp[ry] = 5; if (mp[bytes4(0xdeadbeef)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } mp[bytes4(0xdeadbeef)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[bytes4(0xdeadbeef)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; mp[bytes4(0xdeadbeef)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_bytes_to_storage_bytesN() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } sb = rx; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_retdirect_bytes_to_storage_bytesN() external  returns (uint256) { sb = _s_retdirect_bytes_to_storage_bytesN(); uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_param_bytes_to_storage_bytesN() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_bytes_to_storage_bytesN(rx); }
    function t_mem_bytes_to_storage_bytesN() external  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; sb = ry; uint256 r; assembly { r := sload(sb.slot) } return r; }
}
contract Cb20_0 {
    bytes20 sv;
    uint256 gap;
    struct P { uint8 a; bytes20 f; uint8 c; }
    P p;
    St_b20 q;
    function _s_retdirect_marray_elem() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_marray_elem(bytes20 pv) internal view returns (uint256) { bytes20[] memory a = new bytes20[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_marray_fixed(bytes20 pv) internal view returns (uint256) { bytes20[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_marray_literal(bytes20 pv) internal view returns (uint256) { bytes20[2] memory a = [pv, bytes20(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_marray_literal_dyn(bytes20 pv) internal view returns (uint256) { bytes20[] memory a = new bytes20[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mstruct_field(bytes20 pv) internal view returns (uint256) { St_b20 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mstruct_literal(bytes20 pv) internal view returns (uint256) { St_b20 memory s = St_b20(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mnested(bytes20 pv) internal view returns (uint256) { bytes20[][] memory a = new bytes20[][](1); a[0] = new bytes20[](1); a[0][0] = pv; bytes20[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_storage_scalar(bytes20 pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_storage_packed(bytes20 pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_storage_struct_copy(bytes20 pv) internal  returns (uint256) { St_b20 memory s = St_b20(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[] memory a = new bytes20[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { bytes20[] memory a = new bytes20[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[] memory a = new bytes20[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { bytes20[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[2] memory a = [rx, bytes20(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { bytes20[2] memory a = [_s_retdirect_marray_literal(), bytes20(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[2] memory a = [ry, bytes20(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[] memory a = new bytes20[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { bytes20[] memory a = new bytes20[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[] memory a = new bytes20[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } St_b20 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_b20 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; St_b20 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } St_b20 memory s = St_b20(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_b20 memory s = St_b20(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; St_b20 memory s = St_b20(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[][] memory a = new bytes20[][](1); a[0] = new bytes20[](1); a[0][0] = rx; bytes20[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { bytes20[][] memory a = new bytes20[][](1); a[0] = new bytes20[](1); a[0][0] = _s_retdirect_mnested(); bytes20[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[][] memory a = new bytes20[][](1); a[0] = new bytes20[](1); a[0][0] = ry; bytes20[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } St_b20 memory s = St_b20(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_b20 memory s = St_b20(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; St_b20 memory s = St_b20(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Cb20_1 {
    bytes20[] arr;
    mapping(bytes20 => uint256) mp;
    mapping(uint256 => bytes20) mv;
    bytes20 sb;
    uint256 gap;
    function _s_retdirect_storage_push() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_storage_push(bytes20 pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_storage_array_elem(bytes20 pv) internal  returns (uint256) { arr.push(bytes20(0x1111111111111111111111111111111111111111)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mapping_value_key_dirty(bytes20 pv) internal  returns (uint256) { mp[pv] = 5; if (mp[bytes20(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mapping_key_read(bytes20 pv) internal  returns (uint256) { mp[bytes20(0x1234567890123456789012345678901234567890)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mapping_value_raw(bytes20 pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function _s_retdirect_bytes_to_storage_bytesN() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_bytes_to_storage_bytesN(bytes20 pv) internal  returns (uint256) { sb = pv; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_local_storage_push() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } arr.push(bytes20(0x1111111111111111111111111111111111111111)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(bytes20(0x1111111111111111111111111111111111111111)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; arr.push(bytes20(0x1111111111111111111111111111111111111111)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } mp[rx] = 5; if (mp[bytes20(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[bytes20(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; mp[ry] = 5; if (mp[bytes20(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } mp[bytes20(0x1234567890123456789012345678901234567890)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[bytes20(0x1234567890123456789012345678901234567890)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; mp[bytes20(0x1234567890123456789012345678901234567890)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_bytes_to_storage_bytesN() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } sb = rx; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_retdirect_bytes_to_storage_bytesN() external  returns (uint256) { sb = _s_retdirect_bytes_to_storage_bytesN(); uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_param_bytes_to_storage_bytesN() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_bytes_to_storage_bytesN(rx); }
    function t_mem_bytes_to_storage_bytesN() external  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; sb = ry; uint256 r; assembly { r := sload(sb.slot) } return r; }
}
contract Cen_0 {
    E sv;
    uint256 gap;
    struct P { uint8 a; E f; uint8 c; }
    P p;
    St_en q;
    function _s_retdirect_marray_elem() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_marray_elem(E pv) internal view returns (uint256) { E[] memory a = new E[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_marray_fixed(E pv) internal view returns (uint256) { E[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_marray_literal(E pv) internal view returns (uint256) { E[2] memory a = [pv, E.A]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_marray_literal_dyn(E pv) internal view returns (uint256) { E[] memory a = new E[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mstruct_field(E pv) internal view returns (uint256) { St_en memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mstruct_literal(E pv) internal view returns (uint256) { St_en memory s = St_en(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mnested(E pv) internal view returns (uint256) { E[][] memory a = new E[][](1); a[0] = new E[](1); a[0][0] = pv; E[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_storage_scalar(E pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_storage_packed(E pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_storage_struct_copy(E pv) internal  returns (uint256) { St_en memory s = St_en(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { E rx; assembly { rx := 0x101 } E[] memory a = new E[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { E[] memory a = new E[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[] memory a = new E[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { E rx; assembly { rx := 0x101 } E[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { E[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { E rx; assembly { rx := 0x101 } E[2] memory a = [rx, E.A]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { E[2] memory a = [_s_retdirect_marray_literal(), E.A]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[2] memory a = [ry, E.A]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { E rx; assembly { rx := 0x101 } E[] memory a = new E[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { E[] memory a = new E[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[] memory a = new E[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { E rx; assembly { rx := 0x101 } St_en memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_en memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; St_en memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { E rx; assembly { rx := 0x101 } St_en memory s = St_en(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_en memory s = St_en(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; St_en memory s = St_en(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { E rx; assembly { rx := 0x101 } E[][] memory a = new E[][](1); a[0] = new E[](1); a[0][0] = rx; E[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { E[][] memory a = new E[][](1); a[0] = new E[](1); a[0][0] = _s_retdirect_mnested(); E[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[][] memory a = new E[][](1); a[0] = new E[](1); a[0][0] = ry; E[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { E rx; assembly { rx := 0x101 } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { E rx; assembly { rx := 0x101 } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { E rx; assembly { rx := 0x101 } St_en memory s = St_en(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_en memory s = St_en(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; St_en memory s = St_en(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Cen_1 {
    E[] arr;
    mapping(E => uint256) mp;
    mapping(uint256 => E) mv;
    function _s_retdirect_storage_push() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_storage_push(E pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_storage_array_elem(E pv) internal  returns (uint256) { arr.push(E.A); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mapping_value_key_dirty(E pv) internal  returns (uint256) { mp[pv] = 5; if (mp[E.B] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mapping_key_read(E pv) internal  returns (uint256) { mp[E.B] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mapping_value_raw(E pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_storage_push() external  returns (uint256) { E rx; assembly { rx := 0x101 } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { E rx; assembly { rx := 0x101 } arr.push(E.A); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(E.A); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; arr.push(E.A); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { E rx; assembly { rx := 0x101 } mp[rx] = 5; if (mp[E.B] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[E.B] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; mp[ry] = 5; if (mp[E.B] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { E rx; assembly { rx := 0x101 } mp[E.B] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[E.B] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; mp[E.B] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { E rx; assembly { rx := 0x101 } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cud_0 {
    U sv;
    uint256 gap;
    struct P { uint8 a; U f; uint8 c; }
    P p;
    St_ud q;
    function _s_retdirect_marray_elem() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_elem(U pv) internal view returns (uint256) { U[] memory a = new U[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_fixed(U pv) internal view returns (uint256) { U[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_literal(U pv) internal view returns (uint256) { U[2] memory a = [pv, U.wrap(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_literal_dyn(U pv) internal view returns (uint256) { U[] memory a = new U[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mstruct_field() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mstruct_field(U pv) internal view returns (uint256) { St_ud memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mstruct_literal(U pv) internal view returns (uint256) { St_ud memory s = St_ud(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mnested(U pv) internal view returns (uint256) { U[][] memory a = new U[][](1); a[0] = new U[](1); a[0][0] = pv; U[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_storage_scalar() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_scalar(U pv) internal  returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_packed(U pv) internal  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_struct_copy(U pv) internal  returns (uint256) { St_ud memory s = St_ud(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_marray_elem() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[] memory a = new U[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view returns (uint256) { U[] memory a = new U[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[] memory a = new U[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view returns (uint256) { U[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[2] memory a = [rx, U.wrap(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view returns (uint256) { U[2] memory a = [_s_retdirect_marray_literal(), U.wrap(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[2] memory a = [ry, U.wrap(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[] memory a = new U[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view returns (uint256) { U[] memory a = new U[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[] memory a = new U[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mstruct_field() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } St_ud memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view returns (uint256) { St_ud memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; St_ud memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } St_ud memory s = St_ud(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view returns (uint256) { St_ud memory s = St_ud(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; St_ud memory s = St_ud(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mnested() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[][] memory a = new U[][](1); a[0] = new U[](1); a[0][0] = rx; U[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view returns (uint256) { U[][] memory a = new U[][](1); a[0] = new U[](1); a[0][0] = _s_retdirect_mnested(); U[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mnested(rx); }
    function t_mem_mnested() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[][] memory a = new U[][](1); a[0] = new U[](1); a[0][0] = ry; U[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_storage_scalar() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external  returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external  returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } St_ud memory s = St_ud(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external  returns (uint256) { St_ud memory s = St_ud(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; St_ud memory s = St_ud(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
}
contract Cud_1 {
    U[] arr;
    mapping(U => uint256) mp;
    mapping(uint256 => U) mv;
    function _s_retdirect_storage_push() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_push(U pv) internal  returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_array_elem(U pv) internal  returns (uint256) { arr.push(U.wrap(3)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_mapping_value_key_dirty() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_value_key_dirty(U pv) internal  returns (uint256) { mp[pv] = 5; if (mp[U.wrap(27)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_key_read(U pv) internal  returns (uint256) { mp[U.wrap(27)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_value_raw(U pv) internal  returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_storage_push() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external  returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } arr.push(U.wrap(3)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external  returns (uint256) { arr.push(U.wrap(3)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; arr.push(U.wrap(3)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_mapping_value_key_dirty() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } mp[rx] = 5; if (mp[U.wrap(27)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external  returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[U.wrap(27)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; mp[ry] = 5; if (mp[U.wrap(27)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } mp[U.wrap(27)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external  returns (uint256) { mp[U.wrap(27)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; mp[U.wrap(27)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external  returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
