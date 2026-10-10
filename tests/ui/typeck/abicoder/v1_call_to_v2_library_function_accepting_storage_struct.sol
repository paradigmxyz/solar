// ported-from: test/libsolidity/syntaxTests/abiEncoder/v1_call_to_v2_library_function_accepting_storage_struct.sol
pragma abicoder v1;
import "./auxiliary/v2_library_function_accepting_storage_struct.sol";

contract Test {
    L.Item item;

    function foo() public view {
        L.get(item);
    }
}
