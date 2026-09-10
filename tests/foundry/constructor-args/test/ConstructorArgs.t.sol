// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ConstructorArgs.sol";

contract ConstructorArgsTest {
    function test_ForwardingHelperConstructor() public {
        require(new ForwardingHelperConstructor(41).value() == 83);
        require(new ForwardingHelperConstructor(0).value() == 1);
    }

    ConstructorArgs public c;
    
    uint256 constant TEST_VALUE = 12345;
    address constant TEST_OWNER = address(0xBEEF);

    function queryResult() external pure returns (bytes memory result) {
        result = new bytes(513);
        result[0] = 0x12;
        result[512] = 0x34;
    }

    function test_HeapQueryConstructor() public {
        bytes[] memory queries = new bytes[](2);
        queries[0] = abi.encodeCall(this.queryResult, ());
        queries[1] = queries[0];
        address instance = address(new HeapQueryConstructor(address(this), queries));
        bytes[] memory results = abi.decode(instance.code, (bytes[]));
        require(results.length == 2);
        bytes memory expected = abi.encode(this.queryResult());
        require(keccak256(results[0]) == keccak256(expected));
        require(keccak256(results[1]) == keccak256(expected));
    }

    function buildString() public returns (ConstructorStringArgs, address) {
        return (new ConstructorStringArgs("test", address(this)), address(this));
    }

    function test_InternalStringConstructor() public {
        (ConstructorStringArgs instance, address owner) = buildString();
        require(keccak256(bytes(instance.name())) == keccak256("test"));
        require(instance.owner() == owner);
    }

    function setUp() public {
        c = new ConstructorArgs(TEST_VALUE, TEST_OWNER);
    }

    function test_ValueSet() public view {
        assert(c.value() == TEST_VALUE);
    }

    function test_OwnerSet() public view {
        assert(c.owner() == TEST_OWNER);
    }

    function test_GetValue() public view {
        assert(c.getValue() == TEST_VALUE);
    }

    function test_GetOwner() public view {
        assert(c.getOwner() == TEST_OWNER);
    }
}

contract HeapQueryConstructor {
    constructor(address target, bytes[] memory queries) payable {
        assembly {
            let m := mload(0x40)
            let l := mload(queries)
            let n := shl(5, l)
            let r := add(m, 64)
            let o := add(r, n)
            for { let i := 0 } iszero(eq(i, n)) { i := add(32, i) } {
                let j := mload(add(add(queries, 32), i))
                if iszero(call(gas(), target, selfbalance(), add(j, 32), mload(j), codesize(), 0)) {
                    returndatacopy(m, 0, returndatasize())
                    revert(m, returndatasize())
                }
                mstore(add(r, i), sub(o, r))
                mstore(o, returndatasize())
                returndatacopy(add(o, 32), 0, returndatasize())
                o := and(add(add(o, returndatasize()), 63), not(31))
            }
            mstore(m, 32)
            mstore(add(m, 32), l)
            return(m, sub(o, m))
        }
    }
}

contract ForwardingHelperConstructor {
    uint256 public value;

    constructor(uint256 x) {
        value = forward(x);
    }

    function forward(uint256 x) internal returns (uint256 result) {
        uint256 size = 256 + (x & 31);
        bytes32 expected;
        assembly {
            let fmp := mload(0x40)
            codecopy(0, 0, size)
            mstore(0x40, fmp)
            mstore(0x60, 0)
            expected := keccak256(0, size)
        }
        if (x != 0) result = increment(x) + x;
        else result = increment(x);
        assembly {
            if iszero(eq(expected, keccak256(0, size))) { revert(0, 0) }
        }
    }

    function increment(uint256 x) internal pure returns (uint256) {
        return x + 1;
    }
}

contract ConstructorStringArgs {
    string public name;
    address public owner;
    event NameChanged(string previous, string current);

    constructor(string memory name_, address owner_) {
        setName(name_);
        owner = owner_;
    }

    function setName(string memory name_) internal {
        string memory previous = name;
        name = name_;
        emit NameChanged(previous, name_);
    }
}
