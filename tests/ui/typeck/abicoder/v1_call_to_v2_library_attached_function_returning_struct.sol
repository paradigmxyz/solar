// ported-from: test/libsolidity/syntaxTests/abiEncoder/v1_call_to_v2_library_attached_function_returning_struct.sol
pragma abicoder v1;
import "./auxiliary/v2_library_attached_function_returning_struct.sol";

contract D {
    using L for uint;

    function test() public {
        uint(1).f(); //~ ERROR: the type of return parameter 1, `struct L.Item memory`, is only supported in ABI coder v2
    }
}
