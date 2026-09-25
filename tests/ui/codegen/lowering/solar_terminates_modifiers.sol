//@ compile-flags: -Ogas --emit=bin

// Returning from the external call skips the code after `_` in every modifier
// still running, such as a lock's release, and commits state no revert would.
// A successful exit through `Return.abiEncoded` or a
// `@custom:solar-terminates` function is rejected while such code is pending;
// a reverting exit rolls everything back and is fine.
import {Return} from "solar:core/v1/Return.sol";

contract Test {
    uint256 locked;

    modifier lock() {
        locked = 1;
        _;
        locked = 0;
    }

    modifier enter() {
        locked = 1;
        _;
    }

    /// @custom:solar-terminates
    function finish(string memory s) internal pure {
        Return.abiEncoded(s);
    }

    /// @custom:solar-terminates
    function fail() internal pure {
        revert();
    }

    function forward(string memory s) internal pure {
        finish(s);
    }

    function direct() external lock {
        Return.abiEncoded("x"); //~ ERROR: this returns from the external call and skips the rest of modifier `lock`
    }

    function tagged() external lock {
        finish("x"); //~ ERROR: this call can return from the external call and skip the rest of modifier `lock`
    }

    function transitive() external lock {
        forward("x"); //~ ERROR: this call can return from the external call and skip the rest of modifier `lock`
    }

    function reverting() external lock {
        fail();
    }

    // Nothing runs after `_` in `enter`.
    function unguarded() external enter {
        finish("x");
    }
}
