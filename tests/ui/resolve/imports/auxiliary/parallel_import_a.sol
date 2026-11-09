import {shared} from "./parallel_import_shared.sol";

function a() pure returns (uint256) {
    return shared();
}
