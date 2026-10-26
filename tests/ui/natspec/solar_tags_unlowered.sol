/// @custom:solar-safe
//~^ ERROR: `@custom:solar-safe` must document a contract or a library
pragma solidity >=0.8.0;

/// @custom:solar-saef
//~^ ERROR: unknown tag `@custom:solar-saef`
import "./auxiliary/solar_tags_import.sol";

/// @custom:solar-inline
//~^ ERROR: `@custom:solar-inline` must document a function that internal calls reach
using {identity} for uint256;

function identity(uint256 x) pure returns (uint256) { return x; }

contract Test {
    /// @custom:solar-safe
    //~^ ERROR: `@custom:solar-safe` must document a contract or a library
    using {identity} for uint256;

    function run() external pure {
        assembly {
            /// @custom:solar-saef
            //~^ ERROR: unknown tag `@custom:solar-saef`
            let x := 1
            /// @custom:solar-scratch
            //~^ ERROR: `@custom:solar-scratch` must document a block statement
            { pop(x) }
            /// @custom:solar-safe
            //~^ ERROR: `@custom:solar-safe` must document a contract or a library
            function helper() { }
            helper()
        }
    }
}
