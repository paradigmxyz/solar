// `@custom:solar-cold` marks a function with a body as rarely run. It takes no argument and
// appears once.

abstract contract C {
    /// @custom:solar-cold
    function internal_() internal pure returns (uint256) {
        return 1;
    }

    /// @custom:solar-cold
    function public_() public pure returns (uint256) {
        return 2;
    }

    /// @custom:solar-cold
    function external_() external pure returns (uint256) {
        return 3;
    }

    /// @custom:solar-cold
    //~^ ERROR: `@custom:solar-cold` must document a function with a body
    function declared() external virtual;

    /// @custom:solar-cold
    //~^ ERROR: `@custom:solar-cold` must document a function with a body
    constructor() {}

    /// @custom:solar-cold
    //~^ ERROR: `@custom:solar-cold` must document a function with a body
    modifier m() {
        _;
    }

    /// @custom:solar-cold
    //~^ ERROR: `@custom:solar-cold` must document a function with a body
    uint256 internal v;

    /// @custom:solar-cold always
    //~^ ERROR: `@custom:solar-cold` takes no argument
    function argument() internal pure {}

    /// @custom:solar-cold
    /// @custom:solar-cold
    //~^ ERROR: duplicate `@custom:solar-cold` tag
    function twice() internal pure {}

    function use() external pure returns (uint256) {
        argument();
        twice();
        return internal_() + public_();
    }
}

/// @custom:solar-cold
//~^ ERROR: `@custom:solar-cold` must document a function with a body
contract Tagged {}

contract Statement {
    function f() external pure returns (uint256 x) {
        /// @custom:solar-cold
        //~^ ERROR: `@custom:solar-cold` must document a function with a body
        x = 1;
    }
}
