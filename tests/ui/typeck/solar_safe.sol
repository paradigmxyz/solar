// `@custom:solar-safe` requires that the code a contract runs, through every
// internal call, uses no inline assembly (`memory`) and only checked
// arithmetic (`arithmetic`); naming neither requires only `memory`. Code tagged
// `@custom:solar-trusted` is reviewed instead. External calls run in call
// frames of their own.

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
    uint256 private seeded = seed();

    function seed() private pure returns (uint256 x) {
        unchecked { x = 1 - uint256(2); }
    }

    // A virtual call runs the override.
    function hook() internal override returns (uint256 y) {
        assembly { y := 5 } //~ ERROR: `Safe` is tagged `@custom:solar-safe` but runs inline assembly
    }

    function read(bytes memory b) external pure returns (uint256) {
        return Raw.word(b);
    }

    function reviewed(bytes memory b) external pure returns (uint256) {
        return Reviewed.word(b);
    }

    // A function whose value is taken may be called through the pointer.
    function pointer() external pure returns (uint256) {
        function() internal pure returns (uint256) f = viaPointer;
        return f();
    }

    function viaPointer() internal pure returns (uint256 v) {
        assembly { v := 7 } //~ ERROR: `Safe` is tagged `@custom:solar-safe` but runs inline assembly
    }

    function uncheckedAssembly() external pure returns (uint256 v) {
        unchecked {
            assembly { v := 9 } //~ ERROR: `Safe` is tagged `@custom:solar-safe` but runs inline assembly
        }
    }

    /// @custom:solar-trusted
    function reviewedHere() external pure returns (uint256 v) {
        assembly { v := 8 }
    }
}

function decrement(uint256 a) pure returns (uint256 r) {
    unchecked { r = a - 1; }
}

function halve(uint256 a) pure returns (uint256 r) {
    assembly { r := shr(1, a) } //~ ERROR: `SafeCreation` is tagged `@custom:solar-safe` but runs inline assembly
}

contract DecrementedState {
    uint256 internal inherited = decrement(1);
}

contract HalvedBase {
    constructor(uint256) {}
}

// The creation runs the state variable initializers of every base and the
// arguments of the inheritance specifiers.
/// @custom:solar-safe
contract SafeCreation is DecrementedState, HalvedBase(halve(4)) {}

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

    /// @custom:solar-trusted
    //~^ ERROR: `@custom:solar-trusted` must document a function, a modifier, a contract, or a library
    uint256 internal v;

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

// Trusted code is reviewed, but the overrides its calls dispatch to are chosen
// by the safe contract, so they are checked.
/// @custom:solar-trusted
abstract contract ReviewedBase {
    function run(uint256 a) external returns (uint256) {
        return _hook(a);
    }

    function _hook(uint256 a) internal virtual returns (uint256) {
        return a;
    }
}

/// @custom:solar-safe
contract SafeOverride is ReviewedBase {
    function _hook(uint256 a) internal pure override returns (uint256 r) {
        unchecked { r = a - 1; }
        assembly { mstore(0x40, 0) } //~ ERROR: `SafeOverride` is tagged `@custom:solar-safe` but runs inline assembly
    }
}

abstract contract ModifiedBase {
    modifier m() virtual {
        _;
    }

    /// @custom:solar-trusted
    function f() external m returns (uint256) {
        return 1;
    }
}

/// @custom:solar-safe
contract SafeModifier is ModifiedBase {
    modifier m() override {
        assembly { mstore(0x40, 0) } //~ ERROR: `SafeModifier` is tagged `@custom:solar-safe` but runs inline assembly
        _;
    }
}

/// @custom:solar-trusted
abstract contract ReviewedInit {
    uint256 internal x;

    constructor() {
        _init();
    }

    function _init() internal virtual {}
}

/// @custom:solar-safe arithmetic
contract SafeInit is ReviewedInit {
    function _init() internal override {
        unchecked { x = x - 1; } //~ ERROR: `SafeInit` is tagged `@custom:solar-safe` but runs an `unchecked` block
    }
}

// Every `@custom:solar-safe` tag adds its properties.
/// @custom:solar-safe memory
/// @custom:solar-safe arithmetic
contract TwoTags {
    function f(uint256 a) external pure returns (uint256) {
        unchecked { return a - 1; } //~ ERROR: `TwoTags` is tagged `@custom:solar-safe` but runs an `unchecked` block
    }
}

// A constant that trusted code reads is still traced for the untrusted code
// that reads it too.
function wrapped(uint256 a) pure returns (uint256 r) {
    unchecked { r = a - 1; } //~ ERROR: `SafeConstant` is tagged `@custom:solar-safe` but runs an `unchecked` block
}

/// @custom:solar-safe arithmetic
contract SafeConstant {
    uint256 internal constant K = wrapped(0); //~ ERROR: initial value for constant variable has to be compile-time constant

    /// @custom:solar-trusted
    function reviewed() external pure returns (uint256) {
        return K;
    }

    function open() external pure returns (uint256) {
        return K;
    }
}

// The overrides a constant's initializer dispatches to are traced even when
// only trusted code reads the constant.
/// @custom:solar-trusted
abstract contract ReviewedConstant {
    uint256 internal constant VALUE = _value(); //~ ERROR: initial value for constant variable has to be compile-time constant

    function run() external pure returns (uint256) {
        return VALUE;
    }

    function _value() internal pure virtual returns (uint256) {
        return 1;
    }
}

/// @custom:solar-safe arithmetic
contract SafeConstantOverride is ReviewedConstant {
    function _value() internal pure override returns (uint256 r) {
        unchecked { r = r - 1; } //~ ERROR: `SafeConstantOverride` is tagged `@custom:solar-safe` but runs an `unchecked` block
    }
}

// Trusted code walks each constant's initializer once, so constants that name
// each other end the walk.
/// @custom:solar-safe
contract SafeConstantCycle {
    uint256 internal constant A = B + 1;
    uint256 internal constant B = A + 1;

    /// @custom:solar-trusted
    function f() external pure returns (uint256) {
        return A;
    }
}

// A helper only trusted code calls is not checked itself, but the overrides its own calls
// dispatch to are chosen by the safe contract, so they are.
abstract contract ChainBase {
    /// @custom:solar-trusted
    function top() internal {
        helper();
    }

    function helper() internal {
        _hook();
    }

    function _hook() internal virtual {}
}

/// @custom:solar-safe
contract SafeChain is ChainBase {
    function _hook() internal override {
        assembly { mstore(0x40, 0) } //~ ERROR: `SafeChain` is tagged `@custom:solar-safe` but runs inline assembly
    }

    function run() external {
        top();
    }
}
