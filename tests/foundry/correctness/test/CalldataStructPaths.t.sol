// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

struct CalldataInner {
    uint256 tag;
    uint256[] values;
}

struct CalldataOuter {
    CalldataInner[] items;
    bytes note;
}

contract CalldataStructTarget {
    uint256[] public storedValues;
    uint256 public storedTag;

    event Member(uint256 tag, uint256[] values, bytes note);

    function assignMember(CalldataOuter calldata p, uint256 i) external {
        storedValues = p.items[i].values;
        storedTag = p.items[i].tag;
    }

    function storedValuesLength() external view returns (uint256) {
        return storedValues.length;
    }

    function returnStruct(CalldataOuter calldata p) external pure returns (CalldataOuter memory) {
        return p;
    }

    function emitMember(CalldataOuter calldata p, uint256 i) external {
        emit Member(p.items[i].tag, p.items[i].values, p.note);
    }

    function nested(CalldataOuter calldata p, uint256 i, uint256 j) external pure returns (uint256, uint256, bytes32) {
        return (p.items[i].tag, p.items[i].values[j], keccak256(abi.encode(p.items[i].values)));
    }
}

interface CalldataStructVm {
    function expectEmit(bool, bool, bool, bool) external;
}

contract CalldataStructPathsTest {
    CalldataStructVm constant vm = CalldataStructVm(address(uint160(uint256(keccak256("hevm cheat code")))));

    event Member(uint256 tag, uint256[] values, bytes note);

    CalldataStructTarget target;

    function setUp() public {
        target = new CalldataStructTarget();
    }

    function testAssignNestedCalldataMemberToStorage() public {
        (bool success,) = address(target).call(assignMemberCall());
        assert(success);

        assert(target.storedTag() == 8);
        assert(target.storedValuesLength() == 3);
        assert(target.storedValues(0) == 33);
        assert(target.storedValues(1) == 44);
        assert(target.storedValues(2) == 55);
    }

    function testReturnCalldataStruct() public view {
        (bool success, bytes memory result) = address(target).staticcall(returnStructCall());
        assert(success);
        assert(keccak256(result) == 0x533d2c8f28842cd4e010ef3cc68d7252b056872891773a794873619b93fb109e);
    }

    function testEmitCalldataMembers() public {
        uint256[] memory values = new uint256[](3);
        values[0] = 33;
        values[1] = 44;
        values[2] = 55;
        vm.expectEmit(false, false, false, true);
        emit Member(8, values, hex"1234");
        (bool success,) = address(target).call(emitMemberCall());
        assert(success);
    }

    function testNestedDynamicMember() public view {
        (bool success, bytes memory result) = address(target).staticcall(nestedCall());
        assert(success);

        uint256 tag;
        uint256 value;
        bytes32 hash;
        assembly {
            tag := mload(add(result, 0x20))
            value := mload(add(result, 0x40))
            hash := mload(add(result, 0x60))
        }

        assert(tag == 8);
        assert(value == 55);
        assert(hash == 0x001daeb07e58a149f421ba7a85fbe21b96e855aeb4106cbcd01b2051759c7c2b);
    }

    function assignMemberCall() internal pure returns (bytes memory data) {
        data = new bytes(452);
        assembly {
            mstore(add(data, 0x20), 0xcbd3e6e500000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x40), 0x0000004000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x60), 0)
            mstore(add(data, 0x80), 0x0000004000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0xa0), 0x0000014000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0xc0), 0x0000000100000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0xe0), 0x0000002000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x100), 0x0000000800000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x120), 0x0000004000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x140), 0x0000000300000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x160), 0x0000002100000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x180), 0x0000002c00000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x1a0), 0x0000003700000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x1c0), 0x0000000212340000000000000000000000000000000000000000000000000000)
        }
    }

    function returnStructCall() internal pure returns (bytes memory data) {
        data = new bytes(420);
        assembly {
            mstore(add(data, 0x20), 0x3279414800000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x40), 0x0000002000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x60), 0x0000004000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x80), 0x0000014000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0xa0), 0x0000000100000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0xc0), 0x0000002000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0xe0), 0x0000000800000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x100), 0x0000004000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x120), 0x0000000300000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x140), 0x0000002100000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x160), 0x0000002c00000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x180), 0x0000003700000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x1a0), 0x0000000212340000000000000000000000000000000000000000000000000000)
        }
    }

    function emitMemberCall() internal pure returns (bytes memory data) {
        data = assignMemberCall();
        assembly {
            let word := mload(add(data, 0x20))
            mstore(
                add(data, 0x20),
                or(
                    0x171101f800000000000000000000000000000000000000000000000000000000,
                    and(word, 0x00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
                )
            )
        }
    }

    function nestedCall() internal pure returns (bytes memory data) {
        data = new bytes(484);
        assembly {
            mstore(add(data, 0x20), 0x45b068fa00000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x40), 0x0000006000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x60), 0)
            mstore(add(data, 0x80), 0x0000000200000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0xa0), 0x0000004000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0xc0), 0x0000014000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0xe0), 0x0000000100000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x100), 0x0000002000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x120), 0x0000000800000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x140), 0x0000004000000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x160), 0x0000000300000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x180), 0x0000002100000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x1a0), 0x0000002c00000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x1c0), 0x0000003700000000000000000000000000000000000000000000000000000000)
            mstore(add(data, 0x1e0), 0x0000000212340000000000000000000000000000000000000000000000000000)
        }
    }
}
