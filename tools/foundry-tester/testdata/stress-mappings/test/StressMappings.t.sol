// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StressMappings.sol";

contract StressMappingsTest {
    StressMappings sm;
    
    function setUp() public {
        sm = new StressMappings();
    }
    
    // ========== Single-level mapping tests ==========
    
    function test_UintToUint() public {
        sm.setUintToUint(1, 100);
        assert(sm.getUintToUint(1) == 100);
        assert(sm.uintToUint(1) == 100);
    }
    
    function test_AddressToUint() public {
        address addr = address(0x1234);
        sm.setAddressToUint(addr, 500);
        assert(sm.getAddressToUint(addr) == 500);
        assert(sm.addressToUint(addr) == 500);
    }
    
    function test_Bytes32ToUint() public {
        bytes32 key = keccak256("test");
        sm.setBytes32ToUint(key, 999);
        assert(sm.getBytes32ToUint(key) == 999);
    }
    
    function test_UintToAddress() public {
        address addr = address(0xABCD);
        sm.setUintToAddress(42, addr);
        assert(sm.uintToAddress(42) == addr);
    }
    
    function test_AddressToAddress() public {
        address a = address(0x1111);
        address b = address(0x2222);
        sm.setAddressToAddress(a, b);
        assert(sm.addressToAddress(a) == b);
    }
    
    function test_UintToBool() public {
        sm.setUintToBool(1, true);
        sm.setUintToBool(2, false);
        assert(sm.uintToBool(1) == true);
        assert(sm.uintToBool(2) == false);
    }
    
    function test_AddressToBool() public {
        address addr = address(0x5555);
        sm.setAddressToBool(addr, true);
        assert(sm.addressToBool(addr) == true);
    }
    
    function test_Bytes32ToBytes32() public {
        bytes32 key = bytes32(uint256(1));
        bytes32 value = bytes32(uint256(2));
        sm.setBytes32ToBytes32(key, value);
        assert(sm.bytes32ToBytes32(key) == value);
    }
    
    // ========== Two-level mapping tests ==========
    
    function test_Allowances() public {
        address owner = address(0x1);
        address spender = address(0x2);
        
        sm.setAllowance(owner, spender, 1000);
        assert(sm.getAllowance(owner, spender) == 1000);
        assert(sm.allowances(owner, spender) == 1000);
    }
    
    function test_Matrix() public {
        sm.setMatrix(0, 0, 10);
        sm.setMatrix(0, 1, 20);
        sm.setMatrix(1, 0, 30);
        sm.setMatrix(1, 1, 40);
        
        assert(sm.getMatrix(0, 0) == 10);
        assert(sm.getMatrix(0, 1) == 20);
        assert(sm.getMatrix(1, 0) == 30);
        assert(sm.getMatrix(1, 1) == 40);
    }
    
    function test_AddressUintToBool() public {
        address addr = address(0x3);
        sm.setAddressUintToBool(addr, 1, true);
        sm.setAddressUintToBool(addr, 2, false);
        
        assert(sm.getAddressUintToBool(addr, 1) == true);
        assert(sm.getAddressUintToBool(addr, 2) == false);
    }
    
    function test_RoleBalances() public {
        bytes32 role = keccak256("ADMIN");
        address account = address(0x4);
        
        sm.setRoleBalance(role, account, 500);
        assert(sm.getRoleBalance(role, account) == 500);
    }
    
    // ========== Three-level mapping tests ==========
    
    function test_Permissions() public {
        address user = address(0x5);
        address resource = address(0x6);
        
        sm.setPermission(user, resource, 1, true);
        sm.setPermission(user, resource, 2, false);
        
        assert(sm.hasPermission(user, resource, 1) == true);
        assert(sm.hasPermission(user, resource, 2) == false);
        assert(sm.hasPermission(user, resource, 3) == false); // Default
    }
    
    function test_Cube() public {
        sm.setCube(0, 0, 0, 111);
        sm.setCube(1, 1, 1, 222);
        sm.setCube(2, 2, 2, 333);
        
        assert(sm.getCube(0, 0, 0) == 111);
        assert(sm.getCube(1, 1, 1) == 222);
        assert(sm.getCube(2, 2, 2) == 333);
    }
    
    function test_ComplexThree() public {
        address addr = address(0x7);
        bytes32 key = keccak256("data");
        
        sm.setComplexThree(addr, 10, key, 9999);
        assert(sm.getComplexThree(addr, 10, key) == 9999);
    }
    
    // ========== Four-level mapping tests ==========
    
    function test_FourLevel() public {
        address a1 = address(0x8);
        address a2 = address(0x9);
        
        sm.setFourLevel(a1, a2, 100, 200, 50000);
        assert(sm.getFourLevel(a1, a2, 100, 200) == 50000);
    }
    
    function test_HyperCube() public {
        sm.setHyperCube(1, 2, 3, 4, true);
        sm.setHyperCube(4, 3, 2, 1, false);
        
        assert(sm.getHyperCube(1, 2, 3, 4) == true);
        assert(sm.getHyperCube(4, 3, 2, 1) == false);
        assert(sm.getHyperCube(0, 0, 0, 0) == false); // Default
    }
    
    // ========== Five-level mapping tests ==========
    
    function test_FiveLevel() public {
        address a1 = address(0xA);
        address a2 = address(0xB);
        bytes32 key = keccak256("deep");
        
        sm.setFiveLevel(a1, 1, 2, a2, key, 123456);
        assert(sm.getFiveLevel(a1, 1, 2, a2, key) == 123456);
    }
    
    // ========== Struct mapping tests ==========
    
    function test_SetUser() public {
        address addr = address(0xC);
        sm.setUser(addr, 1000, 12345, true);
        
        (uint256 balance, uint256 lastUpdate, bool active) = sm.getUser(addr);
        assert(balance == 1000);
        assert(lastUpdate == 12345);
        assert(active == true);
    }
    
    function test_SetUserBalance() public {
        address addr = address(0xD);
        sm.setUserBalance(addr, 5000);
        assert(sm.getUserBalance(addr) == 5000);
    }
    
    function test_SetUserActive() public {
        address addr = address(0xE);
        sm.setUserActive(addr, true);
        assert(sm.isUserActive(addr) == true);
        
        sm.setUserActive(addr, false);
        assert(sm.isUserActive(addr) == false);
    }
    
    function test_SetPosition() public {
        sm.setPosition(1, 10, -20);
        (int256 x, int256 y, uint256 ts) = sm.getPosition(1);
        assert(x == 10);
        assert(y == -20);
        assert(ts > 0);
    }
    
    function test_SetUserTokenData() public {
        address addr = address(0xF);
        sm.setUserTokenData(addr, 42, 777, true);
        
        (uint256 balance, uint256 lastUpdate, bool active) = sm.getUserTokenData(addr, 42);
        assert(balance == 777);
        assert(lastUpdate > 0);
        assert(active == true);
    }
    
    // ========== Complex operation tests ==========
    
    function test_IncrementBalance() public {
        address addr = address(0x10);
        sm.setUser(addr, 100, 0, true);
        
        sm.incrementBalance(addr);
        assert(sm.getUserBalance(addr) == 101);
        
        sm.incrementBalance(addr);
        sm.incrementBalance(addr);
        assert(sm.getUserBalance(addr) == 103);
    }
    
    function test_AddToAllowance() public {
        address owner = address(0x11);
        address spender = address(0x12);
        
        sm.setAllowance(owner, spender, 100);
        sm.addToAllowance(owner, spender, 50);
        assert(sm.getAllowance(owner, spender) == 150);
    }
    
    function test_TransferAllowance() public {
        address from = address(0x13);
        address to = address(0x14);
        address spender = address(0x15);
        
        sm.setAllowance(from, spender, 1000);
        sm.setAllowance(to, spender, 200);
        
        sm.transferAllowance(from, to, spender, 300);
        
        assert(sm.getAllowance(from, spender) == 700);
        assert(sm.getAllowance(to, spender) == 500);
    }
    
    function test_SwapMatrixValues() public {
        sm.setMatrix(0, 0, 100);
        sm.setMatrix(1, 1, 200);
        
        sm.swapMatrixValues(0, 0, 1, 1);
        
        assert(sm.getMatrix(0, 0) == 200);
        assert(sm.getMatrix(1, 1) == 100);
    }
    
    function test_BulkSetMatrix() public {
        uint256[] memory rows = new uint256[](3);
        uint256[] memory cols = new uint256[](3);
        uint256[] memory values = new uint256[](3);
        
        rows[0] = 0; cols[0] = 0; values[0] = 11;
        rows[1] = 1; cols[1] = 1; values[1] = 22;
        rows[2] = 2; cols[2] = 2; values[2] = 33;
        
        sm.bulkSetMatrix(rows, cols, values);
        
        assert(sm.getMatrix(0, 0) == 11);
        assert(sm.getMatrix(1, 1) == 22);
        assert(sm.getMatrix(2, 2) == 33);
    }
    
    function test_SumMatrixRow() public {
        sm.setMatrix(0, 0, 10);
        sm.setMatrix(0, 1, 20);
        sm.setMatrix(0, 2, 30);
        sm.setMatrix(0, 3, 40);
        
        uint256 sum = sm.sumMatrixRow(0, 4);
        assert(sum == 100);
    }
    
    function test_CountActiveUsers() public {
        address[] memory addrs = new address[](4);
        addrs[0] = address(0x20);
        addrs[1] = address(0x21);
        addrs[2] = address(0x22);
        addrs[3] = address(0x23);
        
        sm.setUser(addrs[0], 0, 0, true);
        sm.setUser(addrs[1], 0, 0, false);
        sm.setUser(addrs[2], 0, 0, true);
        sm.setUser(addrs[3], 0, 0, true);
        
        uint256 count = sm.countActiveUsers(addrs);
        assert(count == 3);
    }
}
