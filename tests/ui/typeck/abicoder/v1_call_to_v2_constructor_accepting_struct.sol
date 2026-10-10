// ported-from: test/libsolidity/syntaxTests/abiEncoder/v1_call_to_v2_constructor_accepting_struct.sol
pragma abicoder v1;
import "./auxiliary/v2_constructor_accepting_struct.sol";

contract Test {
    function foo() public {
        new C(C.Item(5)); //~ ERROR: the type of this parameter, `struct C.Item memory`, is only supported in ABI coder v2
    }
}
