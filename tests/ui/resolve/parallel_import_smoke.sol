//@ compile-flags: -j8

import "./auxiliary/parallel_import_a.sol" as A;
import "./auxiliary/parallel_import_b.sol" as B;
import "./auxiliary/parallel_import_c.sol" as C;

contract ParallelImportSmoke {
    function f() external pure returns (uint256) {
        return A.a() + B.b() + C.c();
    }
}
