//@ revisions: check abi bin
//@ compile-flags: --allow=2264
//@[abi] compile-flags: --emit=abi
//@[bin] compile-flags: --emit=bin -Ogas
import {Return} from "solar:core/Return.sol";
import {Calls} from "solar:core/Calls.sol";

contract Matching {
    function() internal pure pointer = raw;
    function run() external view returns (uint256) {
        function() internal view f = pointer;
        f();
        return 42;
    }
    function raw() internal pure {
        Return.raw(hex"01"); //~ ERROR: this ends a call to `run` with raw bytes, which only the fallback function returns
    }
}

contract Retyped {
    function run() external pure returns (uint256) {
        function(uint256) internal pure f = noop;
        function(bytes memory) internal pure other = raw;
        assembly { f := other }
        f(0x60);
        return 42;
    }
    function noop(uint256) internal pure {}
    function raw(bytes memory data) internal pure {
        Return.raw(data); //~ ERROR: this ends a call to `run` with raw bytes, which only the fallback function returns
    }
}


contract DelegateWrite {
    function(uint256) internal pure pointer = noop;
    function(int256) internal pure other = raw;

    function run(address target) external returns (uint256) {
        // Delegated code can install other into pointer's slot. Both argument representations
        // are full words, so the generated dispatcher can reach raw after the write.
        (bool success,) = target.delegatecall("");
        require(success);
        pointer(1);
        return 42;
    }
    function noop(uint256) internal pure {}
    function raw(int256) internal pure {
        Return.raw(hex"01"); //~ ERROR: this ends a call to `run` with raw bytes, which only the fallback function returns
    }
}

contract CoreDelegateWrite {
    function(uint256) internal pure pointer = noop;
    function(int256) internal pure other = raw;

    function run(address target) external returns (uint256) {
        // A compiler-owned wrapper still delegates storage access to external code.
        (bool success,,) = Calls.delegateCallInto(target, gasleft(), "", new bytes(0));
        require(success);
        pointer(1);
        return 42;
    }
    function noop(uint256) internal pure {}
    function raw(int256) internal pure {
        Return.raw(hex"01"); //~ ERROR: this ends a call to `run` with raw bytes, which only the fallback function returns
    }
}
