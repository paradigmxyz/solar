// ported-from: test/libsolidity/syntaxTests/inlineAssembly/circular_constant_access_module_err.sol
import "./auxiliary/circular_constant_access_module_err.sol" as M;
uint constant b = M.c;
uint constant d = b;
contract C {
    uint constant a = b;
    function f() public returns (uint t) {
        assembly {
            t := a //~ ERROR: constant variable is circular
        }
    }
}
