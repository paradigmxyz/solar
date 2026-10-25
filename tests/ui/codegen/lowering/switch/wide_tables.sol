//@ revisions: gas size
//@[gas] compile-flags: -O gas -Zpack-wide-jump-tables -Zdump=evm-ir-runtime
//@[size] compile-flags: -O size -Zpack-wide-jump-tables -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

contract WideTables {
    // With `-Zpack-wide-jump-tables`, the planner prices four adjacent keys as a packed table
    // instead of a stride table, so both modes choose it. Without the flag the switch stays a
    // linear scan (see `auto_plans.sol`).
    // CHECK-LABEL: @module WideTables_runtime
    // CHECK: push 4{{$}}
    // CHECK-NEXT: gt
    // CHECK: indexed_jump {{(bb[0-9]+, ){3}bb[0-9]+$}}
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
