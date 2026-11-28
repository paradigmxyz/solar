// `@custom:solar-coverage` leaves code out of coverage or puts it back in. It documents a contract,
// a library, a function with a body, or a modifier, appears once, and takes exactly `off` or `on`.

/// @custom:solar-coverage off
abstract contract C {
    /// @custom:solar-coverage on
    function covered() internal pure returns (uint256) {
        return 1;
    }

    /// @custom:solar-coverage off
    function uncovered() external pure returns (uint256) {
        return 2;
    }

    /// @custom:solar-coverage off
    modifier m() {
        _;
    }

    /// @custom:solar-coverage on
    constructor() {}

    /// @custom:solar-coverage off
    //~^ ERROR: `@custom:solar-coverage` must document a contract, a library, or a function with a body
    function declared() external virtual;

    /// @custom:solar-coverage off
    //~^ ERROR: `@custom:solar-coverage` must document a contract, a library, or a function with a body
    uint256 internal v;

    /// @custom:solar-coverage off
    //~^ ERROR: `@custom:solar-coverage` must document a contract, a library, or a function with a body
    event E();

    /// @custom:solar-coverage
    //~^ ERROR: `@custom:solar-coverage` takes `off` or `on`
    function bare() internal pure {}

    /// @custom:solar-coverage maybe
    //~^ ERROR: `@custom:solar-coverage` takes `off` or `on`
    function unknown() internal pure {}

    /// @custom:solar-coverage off on
    //~^ ERROR: `@custom:solar-coverage` takes `off` or `on`
    function two() internal pure {}

    /// @custom:solar-coverage off
    /// @custom:solar-coverage on
    //~^ ERROR: duplicate `@custom:solar-coverage` tag
    function twice() internal pure {}
}

/// @custom:solar-coverage on
library L {
    function f() internal {}
}

/// @custom:solar-coverage off
//~^ ERROR: `@custom:solar-coverage` must document a contract, a library, or a function with a body
interface I {}

contract Statement {
    function f() external pure returns (uint256 x) {
        /// @custom:solar-coverage off
        //~^ ERROR: `@custom:solar-coverage` must document a contract, a library, or a function with a body
        x = 1;
    }
}
