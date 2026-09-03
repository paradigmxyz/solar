#!/bin/bash
# Re-verify every logged repro against the current solar binary.
cd "$(dirname "$0")/../.."
sym() { # file contract sig [extra...]
  local f=$1 c=$2 s=$3; shift 3
  python3 fuzz/bin/solsymdiff --source "$f" --contract "$c" --signature "$s" --include-view --include-stateful "$@" > /tmp/rc.json 2>/dev/null
  python3 -c "import json; d=json.load(open('/tmp/rc.json')); print('  %-48s %s %s' % ('$s', d['status'], (d.get('reason') or '')[:60]))"
}
echo "1 literal_addmod_fold"; sym symbolic-audit/literal_addmod_fold.sol LiteralAddmodFold 'fold()'; sym symbolic-audit/literal_addmod_fold.sol LiteralAddmodFold 'test()'
echo "2 getter_out_of_bounds"; sym symbolic-audit/getter_out_of_bounds.sol GetterOutOfBounds 'dynamicArray(uint256)'; sym symbolic-audit/getter_out_of_bounds.sol GetterOutOfBounds 'fixedArray(uint256)'
echo "3 assembly_calldata_pointer_encode"; sym symbolic-audit/assembly_calldata_pointer_encode.sol AssemblyCalldataPointerEncode 'encodeStruct()'
echo "4 memory_array_too_large"; sym symbolic-audit/memory_array_too_large.sol MemoryArrayTooLarge 'f()'
echo "5 storage_to_memory_tuple_order"; sym symbolic-audit/storage_to_memory_tuple_order.sol StorageToMemoryTupleOrder 'memorySnapshot()'
echo "6 assembly_calldata_slice_underflow"; sym symbolic-audit/assembly_calldata_slice_underflow.sol AssemblyCalldataSliceUnderflow 'delegate(bytes)'
echo "7 stack_rematerialization_unoptimized (-Onone)"; sym symbolic-audit/stack_rematerialization_unoptimized.sol StackRematerializationUnoptimized 'first(bool)' --no-optimize
echo "8 unused_bound_library_function"; sym symbolic-audit/unused_bound_library_function.sol UnusedBoundLibraryFunction 'f(uint256)'
echo "9 hex_literal_fixed_bytes_constant"; sym symbolic-audit/hex_literal_fixed_bytes_constant.sol HexLiteralFixedBytesConstant 'constantHex()'
echo "10 udvt_dirty_param"; sym symbolic-audit/udvt_dirty_param.sol UdvtDirtyParam 'viaOperator(uint256,uint256)'; sym symbolic-audit/udvt_dirty_param.sol UdvtDirtyParam 'viaCall(uint256,uint256)'; sym symbolic-audit/udvt_dirty_param.sol UdvtDirtyParam 'viaWiden(uint256)'; sym symbolic-audit/udvt_dirty_param.sol UdvtDirtyParam 'viaWidenSigned(uint256)'
echo "11 calldata_static_array_validation (concrete)"; python3 target/symaudit/concrete.py symbolic-audit/calldata_static_array_validation.sol CalldataStaticArrayValidation 'readSecond(uint8[2])' 'raw:0x101 0x1' 'raw:0x1 0x1' 2>&1 | grep -v "Revert$" | head -4; python3 target/symaudit/concrete.py symbolic-audit/calldata_static_array_validation.sol CalldataStaticArrayValidation 'bools(bool[2])' 'raw:0x2 0x1' 2>&1 | grep -v "Revert$" | head -4
echo "12 implicit_widen_alloc_mapping (concrete)"; for s in 'newLength(uint256)' 'newBytesLength(uint256)' 'mappingKey(uint256)' 'signedMappingKey(uint256)'; do python3 target/symaudit/concrete.py symbolic-audit/implicit_widen_alloc_mapping.sol ImplicitWidenAllocMapping "$s" 0x101 0x1ff 2>&1 | grep -v "Revert$" | head -4; done
rm -f /tmp/rc.json
