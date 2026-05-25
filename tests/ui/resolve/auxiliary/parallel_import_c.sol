import {a} from "./parallel_import_a.sol";
import {b} from "./parallel_import_b.sol";

function c() pure returns (uint256) {
    return a() + b();
}
