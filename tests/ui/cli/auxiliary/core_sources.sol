import {Bits} from "solar:core/Bits.sol";
contract C {
    function leading(uint256 x) external pure returns (uint256) {
        return Bits.leadingZeros(x);
    }
}
