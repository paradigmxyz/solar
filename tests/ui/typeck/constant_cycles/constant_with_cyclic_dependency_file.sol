// ported-from: test/libsolidity/syntaxTests/constants/initialization/constant_with_cyclic_dependency_file.sol
import "./auxiliary/constant_with_cyclic_dependency_file.sol";
//~? ERROR: the value of the constant `B` has a cyclic dependency via `A`

uint256 constant A = B + 1; //~ ERROR: the value of the constant `A` has a cyclic dependency via `B`
