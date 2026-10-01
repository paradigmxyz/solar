// `@custom:solar-safe` requires that the code a contract runs, through every
// internal call, uses no inline assembly (`memory`) and only checked
// arithmetic (`arithmetic`); naming neither requires both. The compiler-owned
// modules and code tagged `@custom:solar-trusted` are trusted, and an assembly
// block that only points a storage reference at its ERC-7201 namespace is
// verified instead. External calls run in call frames of their own.
import {Bytes} from "solar:core/Bytes.sol";
import {Math} from "solar:core/Math.sol";

library Raw {
    function word(bytes memory b) internal pure returns (uint256 w) {
        assembly { w := mload(add(b, 32)) } //~ ERROR: `Safe` is tagged `@custom:solar-safe` but runs inline assembly
    }

    function unused() internal pure returns (uint256 w) {
        assembly { w := 1 }
    }
}

/// @custom:solar-trusted
library Reviewed {
    function word(bytes memory b) internal pure returns (uint256 w) {
        assembly { w := mload(add(b, 32)) }
    }
}

contract Base {
    function hook() internal virtual returns (uint256) {
        return 1;
    }

    function run() external returns (uint256) {
        return hook();
    }
}

/// @custom:solar-safe
contract Safe is Base {
    /// @custom:storage-location erc7201:example.main
    struct Main {
        uint256 x;
    }

    bytes32 private constant MAIN = 0x183a6125c38840424c4a85fa12bab2ab606c4b6d0e7cc73c0c06ba5300eab500;

    uint256 private seeded = seed();

    function seed() private pure returns (uint256 x) {
        unchecked { x = 1 - uint256(2); } //~ ERROR: `Safe` is tagged `@custom:solar-safe` but runs an `unchecked` block
    }

    function _main() private pure returns (Main storage $) {
        assembly { $.slot := MAIN }
    }

    // A virtual call runs the override.
    function hook() internal override returns (uint256 y) {
        assembly { y := 5 } //~ ERROR: `Safe` is tagged `@custom:solar-safe` but runs inline assembly
    }

    function read(bytes memory b) external pure returns (uint256, bytes4) {
        return (Raw.word(b), Bytes.readBytes4(b, 0));
    }

    function reviewed(bytes memory b) external pure returns (uint256) {
        return Reviewed.word(b);
    }

    function wrap(uint256 a) external pure returns (uint256) {
        return Math.wrappingAdd(a, 1); //~ ERROR: `Safe` is tagged `@custom:solar-safe` but runs wrapping arithmetic
    }

    // A wrapping operation taken as a value is called through the pointer.
    function wrapPointer(uint256 a) external pure returns (uint256) {
        function(uint256, uint256) internal pure returns (uint256) f = Math.wrappingAdd; //~ ERROR: `Safe` is tagged `@custom:solar-safe` but runs wrapping arithmetic
        return f(a, 1);
    }

    // A function whose value is taken may be called through the pointer.
    function pointer() external pure returns (uint256) {
        function() internal pure returns (uint256) f = viaPointer;
        return f();
    }

    function viaPointer() internal pure returns (uint256 v) {
        assembly { v := 7 } //~ ERROR: `Safe` is tagged `@custom:solar-safe` but runs inline assembly
    }

    /// @custom:solar-trusted
    function reviewedHere() external pure returns (uint256 v) {
        assembly { v := 8 }
    }

    function main() external view returns (uint256) {
        return _main().x;
    }
}

contract WrappingState {
    uint256 internal inherited = Math.wrappingAdd(1, 1); //~ ERROR: `SafeCreation` is tagged `@custom:solar-safe` but runs wrapping arithmetic
}

contract WrappingBase {
    constructor(uint256) {}
}

/// @custom:solar-trusted
contract ReviewedState {
    uint256 internal reviewed = Math.wrappingAdd(2, 2);
}

// The creation runs the state variable initializers of every base and the
// arguments of the inheritance specifiers, outside any function.
/// @custom:solar-safe
contract SafeCreation is WrappingState, ReviewedState, WrappingBase(Math.wrappingMul(2, 3)) { //~ ERROR: `SafeCreation` is tagged `@custom:solar-safe` but runs wrapping arithmetic
    uint256 private seeded = Math.wrappingSub(1, 2); //~ ERROR: `SafeCreation` is tagged `@custom:solar-safe` but runs wrapping arithmetic
    function(uint256, uint256) internal pure returns (uint256) private op = Math.wrappingAdd; //~ ERROR: `SafeCreation` is tagged `@custom:solar-safe` but runs wrapping arithmetic
}

/// @custom:solar-safe memory
contract MemoryOnly {
    function f(uint256 a) external pure returns (uint256) {
        unchecked { return a + 1; }
    }

    function g() external pure returns (uint256 v) {
        assembly { v := 1 } //~ ERROR: `MemoryOnly` is tagged `@custom:solar-safe` but runs inline assembly
    }
}

/// @custom:solar-safe arithmetic
contract ArithmeticOnly {
    function f(uint256 a) external pure returns (uint256) {
        unchecked { return a + 1; } //~ ERROR: `ArithmeticOnly` is tagged `@custom:solar-safe` but runs an `unchecked` block
    }
}

// A library runs all of its functions.
/// @custom:solar-safe
library SafeLib {
    function f() internal pure returns (uint256 v) {
        assembly { v := 1 } //~ ERROR: `SafeLib` is tagged `@custom:solar-safe` but runs inline assembly
    }
}

/// @custom:solar-safe everything
//~^ ERROR: `@custom:solar-safe` has no property `everything`
contract Unknown {}

/// @custom:solar-safe
//~^ ERROR: `@custom:solar-safe` must document a contract or a library
interface Interface {}

contract Misplaced {
    /// @custom:solar-safe
    //~^ ERROR: `@custom:solar-safe` must document a contract or a library
    function f() external {}

    function g() external pure returns (uint256 x) {
        /// @custom:solar-trusted
        //~^ ERROR: `@custom:solar-trusted` must document a function, a modifier, a contract, or a library
        x = 1;
    }
}

contract Other {
    function f() external pure returns (uint256 v) {
        assembly { v := 1 }
    }
}

/// @custom:solar-safe
contract CallsOut {
    function f(Other other) external pure returns (uint256) {
        return other.f();
    }
}
