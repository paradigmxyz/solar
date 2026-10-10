// `Return.abiEncoded`, `Return.raw` and the forwards of `Calls` end the whole
// external call successfully from wherever they run, so each is checked
// against every entry point that can reach it: the value must be the single
// result the entry point declares, raw bytes may only end a call to the
// fallback function, and nothing may end the creation, which would deploy
// what it returns as the code. Only contracts that can be deployed are
// checked, and a call through an internal function pointer can reach every
// function whose value the contract takes.
import {Calls} from "solar:core/Calls.sol";
import {Return} from "solar:core/Return.sol";

type Price is uint256;

contract Checked {
    uint256 private start = _initial();

    constructor() {
        _setUp();
    }

    function _initial() internal pure returns (uint256) {
        Return.abiEncoded(uint256(1)); //~ ERROR: this ends the creation of `Checked` and deploys what it returns as its code
        return 0; //~ WARN: unreachable code
    }

    function _setUp() internal pure {
        Return.raw(""); //~ ERROR: this ends the creation of `Checked` and deploys what it returns as its code
    }

    // Matching results, including through a user-defined value type and a
    // helper shared by two entry points.
    function count() external pure returns (uint256) {
        _count();
    }

    function price() external pure returns (Price) {
        _count();
    }

    function _count() internal pure {
        Return.abiEncoded(uint256(3));
    }

    function label() external pure returns (string memory) {
        _label();
    }

    function title() external pure returns (bytes32) {
        _label();
    }

    function _label() internal pure {
        Return.abiEncoded(string("x")); //~ ERROR: this ends a call to `title` with `(string)`, but it returns `(bytes32)`
    }

    function pair() external pure returns (uint256, uint256) {
        Return.abiEncoded(uint256(1)); //~ ERROR: this ends a call to `pair` with `(uint256)`, but it returns `(uint256,uint256)`
    }

    function nothing() external pure {
        Return.abiEncoded(true); //~ ERROR: this ends a call to `nothing` with `(bool)`, but it returns `()`
    }

    function data() external pure returns (bytes memory) {
        Return.raw(hex"01"); //~ ERROR: this ends a call to `data` with raw bytes, which only the fallback function returns
    }

    function proxied(address target) external returns (bytes memory) {
        Calls.forwardDelegate(target, msg.data); //~ ERROR: this ends a call to `proxied` with raw bytes, which only the fallback function returns
    }

    // `pointed` calls through a pointer, so it can reach `_signed`; `count`
    // makes no pointer call and cannot.
    function pointed() external view returns (address) {
        function() internal pure f = _signed;
        f();
        return address(this);
    }

    function _signed() internal pure {
        Return.abiEncoded(int256(-1)); //~ ERROR: this ends a call to `pointed` with `(int256)`, but it returns `(address)`
    }

    receive() external payable {
        Calls.forward(msg.sender, 0, msg.data); //~ ERROR: this ends a call to the receive function with raw bytes, which only the fallback function returns
    }

    // A fallback function's output is raw bytes: anything may end it.
    fallback() external payable {
        if (msg.data.length == 1) Return.abiEncoded(uint256(4));
        if (msg.data.length == 2) Calls.forward(msg.sender, msg.value, msg.data);
        Return.raw(msg.data);
    }
}

// A contract that is never deployed is not checked; the contracts deriving
// from it are.
abstract contract Base {
    function value() external virtual returns (uint256) {
        Return.abiEncoded(address(0)); //~ ERROR: this ends a call to `value` with `(address)`, but it returns `(uint256)`
    }
}

contract Derived is Base {}

abstract contract Unused {
    function value() external pure returns (uint256) {
        Return.raw("");
    }
}

library Deployed {
    function answer() public pure returns (bool) {
        Return.abiEncoded(bytes32(0)); //~ ERROR: this ends a call to `answer` with `(bytes32)`, but it returns `(bool)`
    }
}

// The operations can only be called directly: a pointer to one would end the
// call where no check follows it.
contract Taken {
    function viaPointer() external pure returns (uint256) { //~ WARN: unnamed return variable can remain unassigned
        function(bytes memory) internal pure f = Return.raw; //~ ERROR: `Return.raw` can only be called directly
        f("");
    }
}
