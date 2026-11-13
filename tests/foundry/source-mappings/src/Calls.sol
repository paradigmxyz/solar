// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IToken {
    function transfer(address to, uint256 amount) external returns (bool);
}

contract Calls {
    function lowLevel(address target, bytes memory data) external returns (bool ok) {
        (bool success, bytes memory ret) = address(target).call(data);
        ok = success && ret.length == 0;
    }

    function precompile(bytes memory input) external view returns (bool ok) {
        (ok,) = address(0x05).staticcall(input);
    }

    function attempt(IToken token, address to) external returns (bool ok) {
        try token.transfer(to, 1) returns (bool result) {
            ok = result;
        } catch {
            ok = false;
        }
    }
}
