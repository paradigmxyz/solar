//@ revisions: bin abi
//@[bin] compile-flags: --emit=bin -Ogas
//@[abi] compile-flags: --emit=abi
import {Buffers, ByteBuilder} from "solar:core/Buffers.sol";
contract Test {
    function f(uint256 n) public pure returns (bytes memory) {
        return helper(Buffers.create(1), n); //~ ERROR: cannot prove builder lifetime through this call
    }
    function helper(ByteBuilder memory b, uint256 n) internal pure returns (bytes memory) {
        if (n == 0) return Buffers.finish(b);
        return helper(b, n - 1);
    }
}
