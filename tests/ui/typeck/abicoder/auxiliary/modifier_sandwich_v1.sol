pragma abicoder v1;
import "./modifier_sandwich_v2.sol";

contract B {
    modifier validate() {
        A(address(0x00)).get();
        _;
    }
}
