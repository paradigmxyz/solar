// ported-from: test/libsolidity/syntaxTests/abiEncoder/v1_call_to_v2_event_accepting_struct.sol
pragma abicoder v1;
import "./auxiliary/v2_event_accepting_struct.sol";

contract Test {
    function foo() public {
        emit L.E(L.Item(42)); //~ ERROR: the type of this parameter, `struct L.Item memory`, is only supported in ABI coder v2
    }
}
