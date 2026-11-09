//@ compile-flags: solar:core/=auxiliary/
import {Bits} from "solar:core/Bits.sol";
contract Test { function f() external pure returns(uint) { return Bits.highestSetBit(1); } }
