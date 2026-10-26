// ported-from: test/libsolidity/syntaxTests/constants/cross_file_cyclic.sol
import "./auxiliary/cross_file_cyclic.sol";
//~? ERROR: the value of the constant `c` has a cyclic dependency via `d`
uint constant b = c; //~ ERROR: the value of the constant `b` has a cyclic dependency via `c`
uint constant d = b; //~ ERROR: the value of the constant `d` has a cyclic dependency via `b`
contract C {
    uint constant a = b; //~ ERROR: the value of the constant `a` has a cyclic dependency via `b`
}
