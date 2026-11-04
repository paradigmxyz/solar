import "./Errors.sol" as S;
import "./Mid.sol" as T;
import "./Errors.sol";

contract Top {
    function viaOwn() public pure {
        revert S.E(1);
    }

    function viaMid() public pure {
        revert T.S.E(2);
    }
}
