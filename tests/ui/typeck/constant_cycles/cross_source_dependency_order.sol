// solc names `Y` for `X`, since this source is parsed first. See TYPECK-010 in
// docs/SOLC_DIVERGENCE.md.
import "./auxiliary/cross_source_dependency_order.sol";
//~? ERROR: the value of the constant `Z` has a cyclic dependency via `Z`
uint constant Y = Y; //~ ERROR: the value of the constant `Y` has a cyclic dependency via `Y`
uint constant X = Z + Y; //~ ERROR: the value of the constant `X` has a cyclic dependency via `Z`
