// solc reports 3558 at the assembly read instead; see TYPECK-010 in docs/SOLC_DIVERGENCE.md.
import "./auxiliary/circular_module_access_err.sol";
//~? ERROR: the value of the constant `x` has a cyclic dependency via `x`
contract C {
    function f() public pure returns (uint t) {
        assembly {
            // Reference to a circular member
            t := x
        }
    }
}
