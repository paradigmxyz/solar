import {shared} from "./parallel_import_shared.sol";

function b() pure returns (uint256) {
    return shared() + 1;
}
