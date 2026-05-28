// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for external call optimizations
/// @dev Solar should optimize ABI encoding and call patterns

interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
    function approve(address spender, uint256 amount) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
    function allowance(address owner, address spender) external view returns (uint256);
}

contract ExternalCall {
    error TransferFailed();
    error ApprovalFailed();

    /// @dev Standard ERC20 transfer call
    /// Optimal: Direct calldata construction without abi.encodeWithSelector
    function safeTransfer(address token, address to, uint256 amount) public {
        (bool success, bytes memory data) = token.call(
            abi.encodeWithSelector(IERC20.transfer.selector, to, amount)
        );
        require(success && (data.length == 0 || abi.decode(data, (bool))), "Transfer failed");
    }

    /// @dev Interface call - compiler should optimize
    function transferViaInterface(IERC20 token, address to, uint256 amount) public {
        bool success = token.transfer(to, amount);
        if (!success) revert TransferFailed();
    }

    /// @dev TransferFrom pattern
    function safeTransferFrom(address token, address from, address to, uint256 amount) public {
        (bool success, bytes memory data) = token.call(
            abi.encodeWithSelector(IERC20.transferFrom.selector, from, to, amount)
        );
        require(success && (data.length == 0 || abi.decode(data, (bool))), "TransferFrom failed");
    }

    /// @dev Approve pattern with return value check
    function safeApprove(address token, address spender, uint256 amount) public {
        (bool success, bytes memory data) = token.call(
            abi.encodeWithSelector(IERC20.approve.selector, spender, amount)
        );
        require(success && (data.length == 0 || abi.decode(data, (bool))), "Approve failed");
    }

    /// @dev View call - staticcall
    function getBalance(address token, address account) public view returns (uint256) {
        return IERC20(token).balanceOf(account);
    }

    /// @dev Staticcall with low-level
    function getBalanceLowLevel(address token, address account) public view returns (uint256) {
        (bool success, bytes memory data) = token.staticcall(
            abi.encodeWithSelector(IERC20.balanceOf.selector, account)
        );
        require(success && data.length >= 32, "Call failed");
        return abi.decode(data, (uint256));
    }

    /// @dev Multiple calls to same contract - optimization opportunity
    function transferAndApprove(
        IERC20 token,
        address recipient,
        uint256 transferAmount,
        address spender,
        uint256 approveAmount
    ) public {
        token.transfer(recipient, transferAmount);
        token.approve(spender, approveAmount);
    }

    /// @dev Call with value
    function callWithValue(address target, bytes calldata data) public payable returns (bytes memory) {
        (bool success, bytes memory result) = target.call{value: msg.value}(data);
        require(success, "Call failed");
        return result;
    }

    /// @dev Delegatecall pattern
    function delegateCall(address target, bytes calldata data) public returns (bytes memory) {
        (bool success, bytes memory result) = target.delegatecall(data);
        require(success, "Delegatecall failed");
        return result;
    }

    /// @dev Batch calls
    function multicall(address[] calldata targets, bytes[] calldata data) 
        public 
        returns (bytes[] memory results) 
    {
        require(targets.length == data.length, "Length mismatch");
        results = new bytes[](targets.length);
        
        for (uint256 i = 0; i < targets.length; i++) {
            (bool success, bytes memory result) = targets[i].call(data[i]);
            require(success, "Call failed");
            results[i] = result;
        }
    }

    /// @dev Try call pattern
    function tryTransfer(IERC20 token, address to, uint256 amount) public returns (bool) {
        try token.transfer(to, amount) returns (bool success) {
            return success;
        } catch {
            return false;
        }
    }
}
