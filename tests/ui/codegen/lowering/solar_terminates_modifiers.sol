//@ compile-flags: -Ogas --emit=bin

// Returning from the external call skips the code after `_` in every modifier
// still running, such as a lock's release, and commits state no revert would.
// A successful exit through `Return.abiEncoded`, `Return.raw`, a forward or a
// `@custom:solar-terminates` function is rejected while such code is pending;
// a reverting exit rolls everything back and is fine.
import {Calls} from "solar:core/Calls.sol";
import {Return} from "solar:core/Return.sol";

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

    function direct() external lock returns (string memory) {
        Return.abiEncoded(string("x")); //~ ERROR: this returns from the external call and skips the rest of modifier `lock`
    }

    function tagged() external lock returns (string memory) {
        finish("x"); //~ ERROR: this call can return from the external call and skip the rest of modifier `lock`
    }

    function transitive() external lock returns (string memory) {
        forward("x"); //~ ERROR: this call can return from the external call and skip the rest of modifier `lock`
    }

    function reverting() external lock {
        fail();
    }

    // A pointer call reaches whatever the pointer can hold.
    function pointer() external lock returns (string memory) {
        function(string memory) internal pure f = finish;
        f("x"); //~ ERROR: this call can return from the external call and skip the rest of modifier `lock`
    }

    function revertingPointer() external lock returns (string memory) {
        function() internal pure f = fail;
        f();
    }

    // Nothing runs after `_` in `enter`.
    function unguarded() external enter returns (string memory) {
        finish("x");
    }

    fallback() external lock {
        Return.raw(msg.data); //~ ERROR: this returns from the external call and skips the rest of modifier `lock`
    }
}

contract Forwarder {
    uint256 locked;

    modifier lock() {
        locked = 1;
        _;
        locked = 0;
    }

    fallback() external payable lock {
        Calls.forward(address(0), msg.value, msg.data); //~ ERROR: this returns from the external call and skips the rest of modifier `lock`
    }
}
