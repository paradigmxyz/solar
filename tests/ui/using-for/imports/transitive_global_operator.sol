//@ compile-flags: -Ztypeck
import "./auxiliary/transitive_mid.sol";

contract C {
    function f(Int a, Int b) public pure returns (Int, Int) {
        return (a + b, -a);
    }
}
