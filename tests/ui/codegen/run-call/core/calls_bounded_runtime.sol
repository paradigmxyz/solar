//@ revisions: gas size portable mir
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[mir] compile-flags: -Ogas -Zdump=mir
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: bounded 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3, 0 => true, 0x, 64
//@ run-call: bounded 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3, 10 => true, 0x11111111111111111111, 64
//@ run-call: bounded 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3, 64 => true, 0x11111111111111111111111111111111111111111111111111111111111111112222222222222222222222222222222222222222222222222222222222222222, 64
//@ run-call: bounded 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3, 100 => true, 0x11111111111111111111111111111111111111111111111111111111111111112222222222222222222222222222222222222222222222222222222222222222, 64
//@ run-call: bounded 0x6029600c60003960296000f37f333333333333333333333333333333333333333333333333333333333333333360005260206000fd, 4 => false, 0x33333333, 32
//@ run-call: boundedStatic 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3, 33 => true, 0x111111111111111111111111111111111111111111111111111111111111111122, 64
//@ run-call: boundedStatic 0x6029600c60003960296000f37f333333333333333333333333333333333333333333333333333333333333333360005260206000fd, 64 => false, 0x3333333333333333333333333333333333333333333333333333333333333333, 32

//@ run-call: echo 5, 36 => true, 0x123456780000000000000000000000000000000000000000000000000000000000000005, 36
//@ run-call: echo 5, 10 => true, 0x12345678000000000000, 36
//@ run-call: echoStatic 5, 6 => true, 0x00000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000006, 64
//@ run-call: nothing 5 => true, 0x, 32

// Bounded calls copy at most `maxCopy` bytes and report the full returndata
// size on success or failure. The identity precompile returns its input, so
// these cases check the payload, truncation and zero-copy boundaries.
import {Calls} from "solar:core/Calls.sol";
import {Create} from "solar:core/Create.sol";

contract Test {
    function bounded(bytes memory initcode, uint256 maxCopy)
        public
        returns (bool success, bytes memory output, uint256 total)
    {
        address target = Create.deploy(initcode, 0);
        return Calls.callBounded(target, 0, gasleft(), "", maxCopy);
    }

    function boundedStatic(bytes memory initcode, uint256 maxCopy)
        public
        returns (bool success, bytes memory output, uint256 total)
    {
        address target = Create.deploy(initcode, 0);
        return Calls.staticCallBounded(target, gasleft(), "", maxCopy);
    }

    function echo(uint256 a, uint256 maxCopy)
        public
        returns (bool success, bytes memory output, uint256 total)
    {
        return
            Calls.callBounded(address(4), 0, gasleft(), abi.encodeWithSelector(0x12345678, a), maxCopy);
    }

    function nothing(uint256 a)
        public
        returns (bool success, bytes memory output, uint256 total)
    {
        return Calls.callBounded(address(4), 0, gasleft(), abi.encode(a), 0);
    }

    function echoStatic(uint256 a, uint256 b)
        public
        view
        returns (bool success, bytes memory output, uint256 total)
    {
        return Calls.staticCallBounded(address(4), gasleft(), abi.encode(a, b), 64);
    }
}
