// ported-from: test/libsolidity/syntaxTests/abiEncoder/v1_call_to_v2_contract_function_returning_dynamic_string_array.sol
pragma abicoder v1;
import "./auxiliary/v2_function_returning_dynamic_string_array.sol";

contract D {
    function g() public view {
        C(address(0x00)).f(); //~ ERROR: the type of return parameter 1, `string[] memory`, is only supported in ABI coder v2
    }
}
