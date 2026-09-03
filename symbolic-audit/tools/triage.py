import json, sys, collections
known_files = {'memory_array_too_large.sol','dirty_storage_array_index.sol','function_type_arrays.sol','253_using_for_function_exists.sol','library_function_attached_but_not_called.sol','creation_code.sol','abi_encode_call_is_consistent_v2.sol','function_array_cross_calls.sol','runtime_code.sol','codesize_data.sol','calldata.sol','external_call_returndata_size.sol','proxy_clobbered_local.sol','external_function_pointer_nested_array.sol','calldata_struct_return.sol','create_memory_array_too_large.sol','selector_expression_side_effect.sol','multi_return_scratch.sol','mir_alloc_ops.sol','multi_return_fmp_clobber.sol','addmod_mulmod.sol','abi_calldata_static_struct_validation.sol','forwarded_calldata_slice_return.sol','storage_tuple_aliasing.sol','stack_only_rematerialization.sol'}
getter_files = {'StressArrays.sol','contract_storage_size_check.sol','mapping_getters.sol','DynamicArray.sol','pretty.sol','variable_access.sol','string_literal_to_fixed_bytes_constant_initialization_1.sol','string_literal_to_fixed_bytes_constant_initialization_2.sol','assembly_local_storage_pointer.sol','arrays_complex_from_and_to_storage.sol','mapping_of_string.sol','array_accessor.sol','chop_sign_bits.sol','string_allocation_bug.sol','arrays_from_and_to_storage.sol','memory_to_storage.sol','delete.sol','accessors_mapping_for_array.sol','storage_reference_array.sol','fixed_bytes_index_access.sol','nested_array_dynamic_static_calldata_to_storage.sol','array_mapping_abstract_constructor_param.sol','calldata_to_storage.sol','inline_assembly_storage_access_local_var.sol','nested_array_dynamic_dynamic_calldata_to_storage.sol','array_mapping_struct.sol','state_variable_dynamic_array.sol','arrays.sol','mapping_array_struct.sol','getters.sol'}
extra_known = set(sys.argv[2:])
for v in sys.argv[1].split(','):
    c=collections.Counter()
    for l in open(f'target/symaudit/results-{v}.jsonl'):
        d=json.loads(l); c[d['status']]+=1
        if d['status']!='mismatch': continue
        f=d['file'].split('/')[-1]
        if f in known_files or f in getter_files or f in extra_known: continue
        print('NEW', v, d['file'], d['contract'], d['signature'], d['mutability'], d['counterexample']['calldata'][:90], d['project'].split('/')[-1])
    print(v, dict(c))
