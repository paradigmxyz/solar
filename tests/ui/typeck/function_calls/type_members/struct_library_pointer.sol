//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/abiEncoder/v2_call_to_v2_library_function_pointer_accepting_struct.sol

import {L} from "./auxiliary/struct_library.sol";

contract Test {
    function foo() public {
        function(L.Item memory) external ptr = L.get; //~ ERROR: mismatched types
        ptr(L.Item(5));
    }
}
