//@ run-call: high() => 0x0000000000000000000000000000000000000000000000001122334455667788
//@ run-call: nextSlot() => 0x0000000000000000000000000000000000000000000000000000000000000000

contract ExternalFunctionPointerStoragePacking {
    function() external fp;
    uint64 x;

    constructor() {
        fp = this.target;
        x = 0x1122334455667788;
    }

    function target() external {}

    function high() external view returns (uint64 value) {
        assembly {
            value := shr(192, sload(0))
        }
    }

    function nextSlot() external view returns (uint64 value) {
        assembly {
            value := sload(1)
        }
    }
}
