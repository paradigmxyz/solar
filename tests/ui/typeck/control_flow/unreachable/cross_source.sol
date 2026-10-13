// Solc merges ranges from different sources into one and drops the part in the other source. We
// report each source's range separately.
import "./auxiliary/cross_source_modifier.sol";
//~? WARN: unreachable code

contract C is B {
    function f() public m returns (uint) {
        revert();
        x = 1; //~ WARN: unreachable code
    }
}
