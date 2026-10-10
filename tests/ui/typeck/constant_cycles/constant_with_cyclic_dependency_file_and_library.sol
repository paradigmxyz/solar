// ported-from: test/libsolidity/syntaxTests/constants/initialization/constant_with_cyclic_dependency_file_and_library.sol
import "./auxiliary/constant_with_cyclic_dependency_file_and_library.sol";
//~? ERROR: the value of the constant `VAL` has a cyclic dependency via `A`

uint256 constant A = B.VAL + 1; //~ ERROR: the value of the constant `A` has a cyclic dependency via `VAL`
