//@ compile-flags: -Onone --emit=srcmap-runtime -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The case of an internal function-pointer dispatcher that jumps to its target is compiler
// generated: the target has not been entered yet, so it has no source location of its own. Only
// the target's body entry carries the target's declaration. Solc maps generated code to its
// `#utility.yul` source (`CompilerContext.h` L178), and LLVM gives a generated thunk no line of its
// own. A function reached through several pointer shapes would otherwise get one stub per
// shape mapped exactly like its real entry, and a coverage anchor could pick a stub that calls
// through another shape never run.
contract C {
    function viaWord(uint256 x) external pure returns (uint256) {
        function(uint256) internal pure returns (uint256) f = target;
        return f(x);
    }

    function viaBytes(uint256 x) external pure returns (uint256) {
        function(uint256) internal pure returns (uint256) f = target;
        bytes32 raw;
        assembly {
            raw := f
        }
        function(bytes32) internal pure returns (bytes32) g;
        assembly {
            g := raw
        }
        return uint256(g(bytes32(x)));
    }

    function target(uint256 a) internal pure returns (uint256) {
        return a + 1;
    }
}

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: bb{{[0-9]+}} [invoke=target@[[TARGET:[0-9]+\.\.[0-9]+]]{{.*}}] !metadata(span=[[TARGET]])
// CHECK-NEXT: push 1 !metadata(span={{[0-9]+\.\.[0-9]+}})
// CHECK: ; internal_dispatcher_p_uint256_r_uint256 (synthesized)
// CHECK-NOT: span=[[TARGET]]
// CHECK: bb{{[0-9]+}}:
// CHECK-NEXT: jump {{bb[0-9]+}} !metadata(compiler_generated)
// CHECK-NOT: span=[[TARGET]]
// CHECK: ; internal_dispatcher_asm_p_bytes32_r_bytes32 (synthesized)
// CHECK-NOT: span=[[TARGET]]
// CHECK: bb{{[0-9]+}}:
// CHECK-NEXT: jump {{bb[0-9]+}} !metadata(compiler_generated)
// CHECK-NOT: span=[[TARGET]]
