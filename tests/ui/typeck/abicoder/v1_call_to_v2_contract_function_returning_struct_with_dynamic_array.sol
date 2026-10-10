// ported-from: test/libsolidity/syntaxTests/abiEncoder/v1_call_to_v2_contract_function_returning_struct_with_dynamic_array.sol
pragma abicoder v1;
import "./auxiliary/v2_function_returning_struct_with_dynamic_array.sol";

contract Test {
    function foo() public view {
        C(address(0x00)).get(); //~ ERROR: the type of return parameter 1, `struct C.Item memory`, is only supported in ABI coder v2
    }
}
