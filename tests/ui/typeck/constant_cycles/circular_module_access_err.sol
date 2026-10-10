// ported-from: test/libsolidity/syntaxTests/inlineAssembly/circular_module_access_err.sol
import "./auxiliary/circular_module_access_err.sol";
contract C {
    function f() public pure returns (uint t) {
        assembly {
            // Reference to a circular member
            t := x //~ ERROR: constant variable is circular
        }
    }
}
