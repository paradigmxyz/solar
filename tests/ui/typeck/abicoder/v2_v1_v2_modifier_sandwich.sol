//~? ERROR: the type of return parameter 1, `struct Data memory`, is only supported in ABI coder v2
// ported-from: test/libsolidity/syntaxTests/abiEncoder/v2_v1_v2_modifier_sandwich.sol
pragma abicoder               v2;

import "./auxiliary/modifier_sandwich_v1.sol";

contract C is B {
    function foo()
        public
        validate()
    {}
}
