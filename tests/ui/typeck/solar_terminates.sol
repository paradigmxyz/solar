// A function tagged `@custom:solar-terminates` ends the call on every path:
// it reverts, halts, or calls another tagged function, and never returns to
// its caller.

contract Test {
    error Bad(uint256 code);

    /// @custom:solar-terminates
    function fail(uint256 code) internal pure {
        revert Bad(code);
    }

    /// @custom:solar-terminates
    function failMessage() internal pure {
        revert("failed");
    }

    /// @custom:solar-terminates
    function branches(uint256 code) private pure returns (uint256) {
        if (code == 0) {
            revert Bad(0);
        } else if (code == 1) {
            failMessage();
        } else {
            fail(code);
        }
    }

    /// @custom:solar-terminates
    function inAssembly() internal pure {
        assembly {
            revert(0, 0)
        }
    }

    /// @custom:solar-terminates
    function giveBack(uint256 code) internal pure returns (uint256) { //~ ERROR: `giveBack` is tagged `@custom:solar-terminates` but can return to its caller
        if (code == 0) revert Bad(0);
        return code;
    }

    /// @custom:solar-terminates
    function fallsThrough(uint256 code) internal pure { //~ ERROR: `fallsThrough` is tagged `@custom:solar-terminates` but can return to its caller
        if (code == 0) revert Bad(0);
    }

    /// @custom:solar-terminates
    function inLoop(uint256 code) internal pure { //~ ERROR: `inLoop` is tagged `@custom:solar-terminates` but can return to its caller
        for (uint256 i; i < code; ++i) {
            revert Bad(i);
        }
    }

    /// @custom:solar-terminates
    function overridable() internal pure virtual {
        revert();
    }

    // A virtual function can be overridden by one that returns.
    /// @custom:solar-terminates
    function throughVirtual() internal pure { //~ ERROR: `throughVirtual` is tagged `@custom:solar-terminates` but can return to its caller
        overridable();
    }

    modifier guard() {
        _;
    }

    /// @custom:solar-terminates
    function guarded() internal pure guard { //~ ERROR: the `@custom:solar-terminates` function `guarded` cannot take modifiers
        revert();
    }

    /// @custom:solar-terminates
    //~^ ERROR: `@custom:solar-terminates` must document an internal or private function
    function entry() external pure {
        revert();
    }

    /// @custom:solar-terminates
    //~^ ERROR: `@custom:solar-terminates` must document an internal or private function
    uint256 x;

    function use(uint256 code) external pure returns (uint256) {
        if (code > 9) {
            return branches(code) + giveBack(code);
        }
        fallsThrough(code);
        inLoop(code);
        throughVirtual();
        guarded();
        inAssembly();
        return code;
    }
}

/// @custom:solar-terminates
//~^ ERROR: `@custom:solar-terminates` must document an internal or private function
contract Tagged {}
