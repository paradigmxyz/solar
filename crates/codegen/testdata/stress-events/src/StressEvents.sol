// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Stress test for many events and emit patterns
/// @notice Tests compiler handling of various event signatures and emit patterns

contract StressEvents {
    // ========== Basic events (no indexed) ==========
    event SimpleUint(uint256 value);
    event SimpleAddress(address addr);
    event SimpleBool(bool flag);
    event SimpleBytes32(bytes32 data);
    event SimpleString(string message);
    
    // ========== Indexed events ==========
    event IndexedUint(uint256 indexed value);
    event IndexedAddress(address indexed addr);
    event IndexedBytes32(bytes32 indexed data);
    
    // ========== Mixed indexed and non-indexed ==========
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event Deposit(address indexed account, uint256 indexed id, uint256 amount);
    event Withdrawal(address indexed account, uint256 amount, uint256 timestamp);
    
    // ========== Multiple non-indexed parameters ==========
    event MultiParam2(uint256 a, uint256 b);
    event MultiParam3(uint256 a, uint256 b, uint256 c);
    event MultiParam4(uint256 a, uint256 b, uint256 c, uint256 d);
    event MultiParam5(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e);
    
    // ========== Maximum indexed (3) ==========
    event ThreeIndexed(uint256 indexed a, uint256 indexed b, uint256 indexed c);
    event ThreeIndexedMixed(address indexed addr, uint256 indexed id, bytes32 indexed hash);
    event ThreeIndexedWithData(address indexed from, address indexed to, uint256 indexed tokenId, uint256 amount);
    
    // ========== Complex data types ==========
    event BytesData(bytes data);
    event StringData(string data);
    event ArrayData(uint256[] values);
    
    // ========== Anonymous events ==========
    event Anonymous1(uint256 value) anonymous;
    event Anonymous2(address indexed addr, uint256 value) anonymous;
    event Anonymous3(uint256 indexed a, uint256 indexed b, uint256 indexed c, uint256 d) anonymous;
    
    // ========== Domain-specific events ==========
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event Paused(address indexed account);
    event Unpaused(address indexed account);
    event RoleGranted(bytes32 indexed role, address indexed account, address indexed sender);
    event RoleRevoked(bytes32 indexed role, address indexed account, address indexed sender);
    
    // ========== DeFi-like events ==========
    event Swap(address indexed sender, uint256 amount0In, uint256 amount1In, uint256 amount0Out, uint256 amount1Out, address indexed to);
    event Sync(uint112 reserve0, uint112 reserve1);
    event Mint(address indexed sender, uint256 amount0, uint256 amount1);
    event Burn(address indexed sender, uint256 amount0, uint256 amount1, address indexed to);
    
    // Storage for testing
    uint256 public counter;
    
    // ========== Emit simple events ==========
    function emitSimpleUint(uint256 value) public {
        emit SimpleUint(value);
    }
    
    function emitSimpleAddress(address addr) public {
        emit SimpleAddress(addr);
    }
    
    function emitSimpleBool(bool flag) public {
        emit SimpleBool(flag);
    }
    
    function emitSimpleBytes32(bytes32 data) public {
        emit SimpleBytes32(data);
    }
    
    function emitSimpleString(string memory message) public {
        emit SimpleString(message);
    }
    
    // ========== Emit indexed events ==========
    function emitIndexedUint(uint256 value) public {
        emit IndexedUint(value);
    }
    
    function emitIndexedAddress(address addr) public {
        emit IndexedAddress(addr);
    }
    
    function emitIndexedBytes32(bytes32 data) public {
        emit IndexedBytes32(data);
    }
    
    // ========== Emit mixed events ==========
    function emitTransfer(address from, address to, uint256 value) public {
        emit Transfer(from, to, value);
    }
    
    function emitApproval(address owner, address spender, uint256 value) public {
        emit Approval(owner, spender, value);
    }
    
    function emitDeposit(address account, uint256 id, uint256 amount) public {
        emit Deposit(account, id, amount);
    }
    
    function emitWithdrawal(address account, uint256 amount) public {
        emit Withdrawal(account, amount, block.timestamp);
    }
    
    // ========== Emit multi-param events ==========
    function emitMultiParam2(uint256 a, uint256 b) public {
        emit MultiParam2(a, b);
    }
    
    function emitMultiParam3(uint256 a, uint256 b, uint256 c) public {
        emit MultiParam3(a, b, c);
    }
    
    function emitMultiParam4(uint256 a, uint256 b, uint256 c, uint256 d) public {
        emit MultiParam4(a, b, c, d);
    }
    
    function emitMultiParam5(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e) public {
        emit MultiParam5(a, b, c, d, e);
    }
    
    // ========== Emit three-indexed events ==========
    function emitThreeIndexed(uint256 a, uint256 b, uint256 c) public {
        emit ThreeIndexed(a, b, c);
    }
    
    function emitThreeIndexedMixed(address addr, uint256 id, bytes32 hash) public {
        emit ThreeIndexedMixed(addr, id, hash);
    }
    
    function emitThreeIndexedWithData(address from, address to, uint256 tokenId, uint256 amount) public {
        emit ThreeIndexedWithData(from, to, tokenId, amount);
    }
    
    // ========== Emit complex data events ==========
    function emitBytesData(bytes memory data) public {
        emit BytesData(data);
    }
    
    function emitStringData(string memory data) public {
        emit StringData(data);
    }
    
    function emitArrayData(uint256[] memory values) public {
        emit ArrayData(values);
    }
    
    // ========== Emit anonymous events ==========
    function emitAnonymous1(uint256 value) public {
        emit Anonymous1(value);
    }
    
    function emitAnonymous2(address addr, uint256 value) public {
        emit Anonymous2(addr, value);
    }
    
    function emitAnonymous3(uint256 a, uint256 b, uint256 c, uint256 d) public {
        emit Anonymous3(a, b, c, d);
    }
    
    // ========== Emit domain events ==========
    function emitOwnershipTransferred(address previousOwner, address newOwner) public {
        emit OwnershipTransferred(previousOwner, newOwner);
    }
    
    function emitPaused(address account) public {
        emit Paused(account);
    }
    
    function emitUnpaused(address account) public {
        emit Unpaused(account);
    }
    
    function emitRoleGranted(bytes32 role, address account, address sender) public {
        emit RoleGranted(role, account, sender);
    }
    
    function emitRoleRevoked(bytes32 role, address account, address sender) public {
        emit RoleRevoked(role, account, sender);
    }
    
    // ========== Emit DeFi events ==========
    function emitSwap(address sender, uint256 amount0In, uint256 amount1In, uint256 amount0Out, uint256 amount1Out, address to) public {
        emit Swap(sender, amount0In, amount1In, amount0Out, amount1Out, to);
    }
    
    function emitSync(uint112 reserve0, uint112 reserve1) public {
        emit Sync(reserve0, reserve1);
    }
    
    function emitMint(address sender, uint256 amount0, uint256 amount1) public {
        emit Mint(sender, amount0, amount1);
    }
    
    function emitBurn(address sender, uint256 amount0, uint256 amount1, address to) public {
        emit Burn(sender, amount0, amount1, to);
    }
    
    // ========== Multiple events in one function ==========
    function emitMultipleEvents() public {
        emit SimpleUint(1);
        emit SimpleUint(2);
        emit SimpleUint(3);
    }
    
    function emitChainedEvents(address from, address to, uint256 value) public {
        emit Transfer(from, to, value);
        emit Approval(from, to, value);
        emit Deposit(from, value, value);
    }
    
    // ========== Conditional event emission ==========
    function emitConditional(uint256 value, bool flag) public {
        if (flag) {
            emit SimpleUint(value);
        } else {
            emit SimpleBool(flag);
        }
    }
    
    // ========== Loop event emission ==========
    function emitInLoop(uint256 count) public {
        for (uint256 i = 0; i < count; i++) {
            emit SimpleUint(i);
        }
    }
    
    // ========== Event with state change ==========
    function incrementAndEmit() public {
        counter++;
        emit SimpleUint(counter);
    }
    
    function multipleStateChangesAndEvents(uint256 a, uint256 b) public {
        counter += a;
        emit SimpleUint(counter);
        counter += b;
        emit SimpleUint(counter);
    }
}
