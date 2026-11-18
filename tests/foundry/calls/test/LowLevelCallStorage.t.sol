// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/LowLevelCallStorage.sol";

contract LowLevelCallStorageTest {
    Reverter internal target;
    StoredCallResults internal stored;

    function setUp() public {
        target = new Reverter();
        stored = new StoredCallResults();
    }

    function test_StoresRevertData() public {
        stored.call(address(target), abi.encodeCall(Reverter.fail, ()));
        assert(!stored.ok());
        assert(keccak256(stored.result()) == keccak256(abi.encodeWithSelector(Reverter.Failed.selector, 7)));
    }

    function test_StoresReturnData() public {
        stored.call(address(target), abi.encodeCall(Reverter.echo, (42)));
        assert(stored.ok());
        assert(keccak256(stored.result()) == keccak256(abi.encode(uint256(42))));
    }

    function test_StoresDataWhenSuccessIsSkipped() public {
        stored.callSkippingSuccess(address(target), abi.encodeCall(Reverter.fail, ()));
        assert(keccak256(stored.result()) == keccak256(abi.encodeWithSelector(Reverter.Failed.selector, 7)));
    }

    function test_StoresStaticCallData() public {
        stored.staticCall(address(target), abi.encodeCall(Reverter.echo, (9)));
        assert(stored.ok());
        assert(keccak256(stored.result()) == keccak256(abi.encode(uint256(9))));
    }

    function test_StoresDelegateCallData() public {
        stored.delegateCall(address(target), abi.encodeCall(Reverter.fail, ()));
        assert(!stored.ok());
        assert(keccak256(stored.result()) == keccak256(abi.encodeWithSelector(Reverter.Failed.selector, 7)));
    }
}
