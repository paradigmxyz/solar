//@ revisions: gas size jit
//@[gas] compile-flags: -O gas -Zpack-wide-jump-tables -Zdump=evm-ir-runtime
//@[size] compile-flags: -O size -Zpack-wide-jump-tables -Zdump=evm-ir-runtime
//@[jit] compile-flags: -O gas -Ztune=jit -Zpack-wide-jump-tables -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefixes=CHECK,PACKED
//@[size] filecheck: --check-prefixes=CHECK,PACKED
//@[jit] filecheck: --check-prefixes=CHECK,JIT
//@ normalize-stdout-test: "(?s).+" -> ""

contract WideTables {
    // With `-Zpack-wide-jump-tables`, the planner prices four adjacent keys as a packed table
    // instead of a stride table, so both modes choose it. Without the flag the switch stays a
    // linear scan (see `auto_plans.sol`), which `-Ztune=jit` also keeps with the flag.
    // CHECK-LABEL: @module WideTables_runtime
    // PACKED: push 4{{$}}
    // PACKED-NEXT: gt
    // PACKED: indexed_jump {{(bb[0-9]+, ){3}bb[0-9]+$}}
    // JIT-NOT: indexed_jump
    // JIT: sub
    // JIT-NOT: indexed_jump
    fallback() external {
        assembly {
            switch calldataload(0)
            case 0 { return(0, 1) }
            case 1 { return(0, 2) }
            case 2 { return(0, 3) }
            case 3 { return(0, 4) }
            default { revert(0, 0) }
        }
    }
}
