// solc reports 3558 at the assembly read instead; see TYPECK-010 in docs/SOLC_DIVERGENCE.md.
import "./auxiliary/circular_constant_access_module_err.sol" as M;
//~? ERROR: the value of the constant `c` has a cyclic dependency via `d`
uint constant b = M.c; //~ ERROR: the value of the constant `b` has a cyclic dependency via `c`
uint constant d = b; //~ ERROR: the value of the constant `d` has a cyclic dependency via `b`
contract C {
    uint constant a = b; //~ ERROR: the value of the constant `a` has a cyclic dependency via `b`
    function f() public returns (uint t) {
        assembly {
            t := a
        }
    }
}
