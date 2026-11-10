// Taking an internal function's address does not execute it in the constructor or caller.
import {Return} from "solar:core/Return.sol";
contract Test {
    function() internal pure pointer = exit;
    function store() external { pointer = exit; }
    function answer() external view returns (uint256 result) { pointer(); }
    function exit() internal pure { Return.abiEncoded(uint256(42)); }
}

library Recursive {
    struct Node { Node[] children; }
    function answer() public pure returns (Node storage) {
        // A storage reference returns a slot word, never a recursively expanded ABI tuple.
        Return.abiEncoded(uint256(42));
    }
    function wrong() public pure returns (Node storage) {
        Return.abiEncoded(true); //~ ERROR: this ends a call to `wrong` with `(bool)`, but it returns `(uint256)`
    }
}
