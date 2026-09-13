// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/OverloadLib.sol";
import "../src/SafeMath.sol";

contract OverloadLibTest {
    function chooseLibrary(bool first) external pure returns (address) {
        if (first) return address(OverloadLib);
        return address(SafeMath);
    }

    function testDistinctLibraryBranches() public {
        require(this.chooseLibrary(true) == address(OverloadLib));
        require(this.chooseLibrary(false) == address(SafeMath));
        require(this.chooseLibrary(true) != this.chooseLibrary(false));
    }

    function testEmbeddedLibraryRuntime() public {
        LibraryAddressChild child = new LibraryAddressChild();
        require(child.linked() == address(OverloadLib));
        RuntimeCodeDeployer deployed = new RuntimeCodeDeployer(type(LibraryAddressChild).runtimeCode);
        require(LibraryAddressChild(address(deployed)).linked() == address(OverloadLib));
    }

    function libraryAddress() external pure returns (address) {
        return address(OverloadLib);
    }

    function testLibraryAddressExpressions() public {
        address linked = this.libraryAddress();
        require(linked.code.length != 0);
        require(uint160(address(OverloadLib)) + 1 == uint160(linked) + 1);
        require(bytes1(bytes20(address(OverloadLib))) == bytes1(bytes20(linked)));
        require(address(OverloadLib) != address(0));
        bytes memory packed = abi.encodePacked(address(OverloadLib));
        require(keccak256(packed) == keccak256(abi.encodePacked(linked)));
    }

    function libraryData() external pure returns (bytes memory) {
        return abi.encode(
            address(OverloadLib),
            uint256(0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0),
            uint256(0xabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789),
            uint256(0x9876543210fedcba9876543210fedcba9876543210fedcba9876543210fedcba),
            uint256(0xfedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210)
        );
    }

    function testLibraryAddressConstantData() public {
        address linked = this.libraryAddress();
        bytes memory data = this.libraryData();
        require(abi.decode(data, (address)) == linked);
    }

    /// @notice Test single-arg find which calls two-arg overload
    function testFindSingleArg() public pure {
        uint256 result = OverloadLib.find(42);
        require(result == 42, "find(42) should return 42");
    }

    /// @notice Test two-arg find directly
    function testFindTwoArg() public pure {
        uint256 result = OverloadLib.find(42, true);
        require(result == 42, "find(42, true) should return 42");

        result = OverloadLib.find(42, false);
        require(result == 0, "find(42, false) should return 0");
    }

    /// @notice Test chained overload resolution
    function testFindDefault() public pure {
        uint256 result = OverloadLib.findDefault(100);
        require(result == 100, "findDefault(100) should return 100");
    }
}

contract LibraryAddressChild {
    function linked() external pure returns (address) { return address(OverloadLib); }
}

contract RuntimeCodeDeployer {
    constructor(bytes memory code) {
        assembly { return(add(code, 32), mload(code)) }
    }
}
