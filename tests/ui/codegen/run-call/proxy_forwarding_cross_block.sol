//@ revisions: gas size
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: ForwardingHarness::fixedRange() => 1
//@ run-call: ForwardingHarness::crossBlock() => 1
//@ run-call: SharedPublicFrame::entry() => 42

contract ForwardingImplementation {
    address private proxyImplementationSlot;
    uint256 public value;

    function set(uint256 a, uint256 b, uint256 c, bytes calldata payload) external {
        value = a + b + c + payload.length;
    }
}

contract FixedRangeProxy {
    address internal implementation;

    constructor(address target) {
        implementation = target;
    }

    fallback() external {
        address target = implementation;
        assembly {
            // This starts in low memory but extends beyond the old 0x2080
            // ownership cutoff. The complete range belongs to this image.
            calldatacopy(0, 0, 0x3000)
            if iszero(delegatecall(gas(), target, 0, calldatasize(), 0, 0)) {
                returndatacopy(0, 0, returndatasize())
                revert(0, returndatasize())
            }
            returndatacopy(0, 0, returndatasize())
            return(0, returndatasize())
        }
    }
}

contract CrossBlockProxy {
    address internal implementation;

    constructor(address target) {
        implementation = target;
    }

    fallback() external {
        address target = implementation;
        if (msg.data.length > 128) {
            assembly {
                calldatacopy(0, 0, calldatasize())
            }
        } else {
            assembly {
                calldatacopy(0, 0, calldatasize())
            }
        }
        assembly {
            if iszero(delegatecall(gas(), target, 0, calldatasize(), 0, 0)) {
                returndatacopy(0, 0, returndatasize())
                revert(0, returndatasize())
            }
            returndatacopy(0, 0, returndatasize())
            return(0, returndatasize())
        }
    }
}

contract ForwardingHarness {
    function fixedRange() external returns (uint256) {
        ForwardingImplementation implementation = new ForwardingImplementation();
        FixedRangeProxy proxy = new FixedRangeProxy(address(implementation));
        ForwardingImplementation(address(proxy)).set(1, 2, 3, new bytes(160));
        require(ForwardingImplementation(address(proxy)).value() == 166, "fixed");
        return 1;
    }

    function crossBlock() external returns (uint256) {
        ForwardingImplementation implementation = new ForwardingImplementation();
        CrossBlockProxy proxy = new CrossBlockProxy(address(implementation));
        ForwardingImplementation(address(proxy)).set(4, 5, 6, new bytes(160));
        require(ForwardingImplementation(address(proxy)).value() == 175, "cross block");
        return 1;
    }
}

contract SharedPublicFrame {
    function helper(uint256 value) public pure returns (uint256) {
        assembly {
            calldatacopy(0, 0, 0x1000)
        }
        return value;
    }

    function entry() external pure returns (uint256) {
        return helper(42);
    }
}
