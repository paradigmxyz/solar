// ported-from: test/libsolidity/syntaxTests/abiEncoder/v1_call_to_v2_contract_function_accepting_struct_via_named_argument.sol
pragma abicoder v1;
import "./auxiliary/v2_function_accepting_struct_via_named_argument.sol";

contract Test {
    function foo() public view {
        C(address(0x00)).set({_item: C.Item(50), _z: false, _y: "abc", _x: 30}); //~ ERROR: the type of this parameter, `struct C.Item memory`, is only supported in ABI coder v2
    }
}
