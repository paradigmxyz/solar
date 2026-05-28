// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for ETH transfer optimizations
/// @dev Solar should use codesize() trick for zero-length call data

contract ETHTransfer {
    error TransferFailed();

    /// @dev Basic ETH transfer with call
    /// Optimal: call(gas(), to, amount, codesize(), 0, codesize(), 0)
    function transferETH(address to, uint256 amount) public {
        (bool success,) = to.call{value: amount}("");
        if (!success) revert TransferFailed();
    }

    /// @dev Transfer all ETH
    /// Optimal: call(gas(), to, selfbalance(), codesize(), 0, codesize(), 0)
    function transferAllETH(address to) public {
        (bool success,) = to.call{value: address(this).balance}("");
        if (!success) revert TransferFailed();
    }

    /// @dev Transfer with gas stipend
    function transferETHWithStipend(address to, uint256 amount, uint256 gasLimit) public {
        (bool success,) = to.call{value: amount, gas: gasLimit}("");
        if (!success) revert TransferFailed();
    }

    /// @dev Try transfer (non-reverting)
    function tryTransferETH(address to, uint256 amount) public returns (bool success) {
        (success,) = to.call{value: amount}("");
    }

    /// @dev Using transfer() - 2300 gas stipend
    function transferETHViaTransfer(address payable to, uint256 amount) public {
        to.transfer(amount);
    }

    /// @dev Using send() - 2300 gas stipend, returns bool
    function transferETHViaSend(address payable to, uint256 amount) public returns (bool) {
        return to.send(amount);
    }

    /// @dev Payable function that forwards ETH
    function forwardETH(address to) public payable {
        (bool success,) = to.call{value: msg.value}("");
        if (!success) revert TransferFailed();
    }

    /// @dev Batch ETH transfer
    function batchTransferETH(address[] calldata recipients, uint256[] calldata amounts) public {
        require(recipients.length == amounts.length, "Length mismatch");
        
        for (uint256 i = 0; i < recipients.length; i++) {
            (bool success,) = recipients[i].call{value: amounts[i]}("");
            if (!success) revert TransferFailed();
        }
    }

    /// @dev Refund pattern - common in gas optimization
    function executeAndRefund(address target, bytes calldata data) public payable {
        uint256 gasBefore = gasleft();
        
        (bool success,) = target.call{value: msg.value}(data);
        require(success, "Call failed");
        
        // Refund excess gas
        uint256 gasUsed = gasBefore - gasleft();
        uint256 refund = msg.value - (gasUsed * tx.gasprice);
        if (refund > 0) {
            (bool refundSuccess,) = msg.sender.call{value: refund}("");
            // Ignore refund failure
        }
    }

    receive() external payable {}
}
