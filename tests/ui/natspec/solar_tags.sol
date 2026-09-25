// `@custom:solar-*` tags state requirements this compiler checks, which other
// compilers read as documentation. A tag in the wrong place or with an unknown
// name is an error rather than a requirement that silently goes unchecked;
// other `@custom:` tags stay documentation.
import {Bytes} from "solar:core/v1/Bytes.sol";

/// @custom:solar-view
//~^ ERROR: `@custom:solar-view` must document a variable declaration statement
contract Test {
    /// @custom:solar-scratch
    //~^ ERROR: `@custom:solar-scratch` must document a block statement
    function f(bytes memory b) public pure returns (uint256 x) {
        /// @custom:solar-view
        //~^ ERROR: `@custom:solar-view` must document a variable declaration statement
        x = 1;
        /// @custom:solar-veiw
        //~^ ERROR: unknown Solar tag `@custom:solar-veiw`
        uint256 y = 2;
        /// @custom:solar-view
        uint256 z = 3; //~ ERROR: `@custom:solar-view` requires a `bytes memory` variable initialized by `Bytes.slice`
        /// @custom:solar-view
        bytes memory c = new bytes(3); //~ ERROR: `@custom:solar-view` requires a `bytes memory` variable initialized by `Bytes.slice`
        /// @custom:solar-view
        bytes memory d; //~ ERROR: `@custom:solar-view` requires a `bytes memory` variable initialized by `Bytes.slice`
        /// @custom:unrelated
        uint256 w = 4;
        /// @custom:solar-view
        bytes memory v = Bytes.slice(b, 0, 1);
        x += y + z + c.length + d.length + w + v.length;
        /// @custom:solar-scratch
        //~^ ERROR: `@custom:solar-scratch` must document a block statement
        x += 1;
        /// @custom:solar-terminates
        //~^ ERROR: `@custom:solar-terminates` must document an internal or private function
        x += 2;
        /// @custom:solar-scratch
        {
            x += abi.encode(x).length;
        }
        /// @custom:solar-scratch
        unchecked {
            x += abi.encode(x).length;
        }
        /// @custom:solar-scratch
        {
            assembly {
                //~^ ERROR: a `@custom:solar-scratch` block cannot contain inline assembly
                x := add(x, 1)
            }
        }
    }

    /// @custom:solar-terminates
    //~^ ERROR: `@custom:solar-terminates` must document an internal or private function
    function g() external pure {
        revert();
    }

    /// @custom:solar-terminates
    function h() internal pure {
        revert();
    }

    /// @custom:solar-terminates
    //~^ ERROR: `@custom:solar-terminates` must document an internal or private function
    modifier m() {
        _;
    }
}
