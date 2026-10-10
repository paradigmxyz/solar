pragma abicoder v1;
import "./auxiliary/v2_error_accepting_struct.sol";

contract C {
    function f(bool c) public pure {
        require(c, E(Item(1))); //~ ERROR: the type of this parameter, `struct Item memory`, is only supported in ABI coder v2
    }

    function g() public pure {
        revert E(Item(1)); //~ ERROR: the type of this parameter, `struct Item memory`, is only supported in ABI coder v2
    }
}
