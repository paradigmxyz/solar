// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/TupleStorageAssign.sol";

contract TupleStorageAssignTest {
    TupleSource internal source;
    TupleStorageAssign internal stored;

    bytes internal constant LONG = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122";

    function setUp() public {
        source = new TupleSource();
        stored = new TupleStorageAssign();
    }

    function check(bytes memory expected) internal view {
        assert(keccak256(stored.result()) == keccak256(expected));
    }

    function test_CallStoresRevertData() public {
        stored.call(address(source), abi.encodeCall(TupleSource.fail, ()));
        assert(!stored.ok());
        check(abi.encodeWithSelector(TupleSource.Failed.selector, 7));
    }

    function test_CallStoresReturnData() public {
        stored.call(address(source), abi.encodeCall(TupleSource.echo, (42)));
        assert(stored.ok());
        check(abi.encode(uint256(42)));
    }

    function test_CallStoresLongReturnData() public {
        stored.call(address(source), abi.encodeCall(TupleSource.pair, ()));
        assert(stored.ok());
        check(abi.encode(uint256(5), LONG));
    }

    function test_CallStoresEmptyReturnData() public {
        stored.call(address(source), abi.encodeCall(TupleSource.echo, (1)));
        stored.call(address(0xdead), "");
        assert(stored.ok());
        check("");
    }

    function test_CallSkippingSuccess() public {
        stored.callSkippingSuccess(address(source), abi.encodeCall(TupleSource.fail, ()));
        assert(!stored.ok());
        check(abi.encodeWithSelector(TupleSource.Failed.selector, 7));
    }

    function test_StaticCall() public {
        stored.staticCall(address(source), abi.encodeCall(TupleSource.echo, (9)));
        assert(stored.ok());
        check(abi.encode(uint256(9)));
    }

    function test_StaticCallRevert() public {
        stored.staticCall(address(source), abi.encodeCall(TupleSource.fail, ()));
        assert(!stored.ok());
        check(abi.encodeWithSelector(TupleSource.Failed.selector, 7));
    }

    function test_DelegateCall() public {
        stored.delegateCall(address(source), abi.encodeCall(TupleSource.echo, (11)));
        assert(stored.ok());
        check(abi.encode(uint256(11)));
    }

    function test_DelegateCallRevert() public {
        stored.delegateCall(address(source), abi.encodeCall(TupleSource.fail, ()));
        assert(!stored.ok());
        check(abi.encodeWithSelector(TupleSource.Failed.selector, 7));
    }

    function test_AbiDecode() public {
        stored.decode(abi.encode(uint256(3), LONG));
        assert(stored.number() == 3);
        check(LONG);
    }

    function test_ExternalCallResult() public {
        stored.externalPair(source);
        assert(stored.number() == 5);
        check(LONG);
    }

    function test_ExternalCallString() public {
        stored.externalText(source);
        assert(keccak256(bytes(stored.message())) == keccak256("a string that is longer than thirty-one bytes"));
        assert(stored.number() == 9);
    }
}
