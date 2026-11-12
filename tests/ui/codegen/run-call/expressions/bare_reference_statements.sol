//@ codegen-matrix: standard
//@ run-call: references; value=5 => 5, 0
//@ run-call: receiverEffects => 9
//@ run-call: tupleEffects => 2
//@ run-call: moduleSelector => 0x26121ff0
//@ run-call-fail: revertingReceiver => 0x
//@ run-call-fail: storageReferences => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: 0x; value=7

import "./auxiliary/bare_reference_module.sol" as Module;

interface IToken {
    function transfer(address to, uint256 amount) external returns (bool);
}

contract BareReferenceStatements {
    event Withdrawn(uint256 amount);
    error Failed();

    uint256 internal constant MAX = type(uint256).max;
    string internal constant NAME = "bare";

    struct Entry {
        uint256 value;
    }

    uint256 private count;
    Entry[] private entries;

    function references() external payable returns (uint256, uint256) {
        address payable to = payable(address(0xdead));
        block; //~ WARN: statement has no effect
        msg; //~ WARN: statement has no effect
        tx; //~ WARN: statement has no effect
        abi; //~ WARN: statement has no effect
        Withdrawn; //~ WARN: statement has no effect
        Failed; //~ WARN: statement has no effect
        IToken; //~ WARN: statement has no effect
        Module; //~ WARN: statement has no effect
        Module.helper; //~ WARN: statement has no effect
        Module.IModule.f; //~ WARN: statement has no effect
        Module.IModule.f.selector; //~ WARN: statement has no effect
        type(IToken); //~ WARN: statement has no effect
        MAX; //~ WARN: statement has no effect
        BareReferenceStatements.MAX; //~ WARN: statement has no effect
        NAME; //~ WARN: statement has no effect
        new uint256[]; //~ WARN: statement has no effect
        to.transfer; //~ WARN: statement has no effect
        to.send; //~ WARN: statement has no effect
        to.call; //~ WARN: statement has no effect
        to.delegatecall; //~ WARN: statement has no effect
        to.staticcall; //~ WARN: statement has no effect
        abi.encode; //~ WARN: statement has no effect
        abi.decode; //~ WARN: statement has no effect
        (block, msg.sender, Withdrawn); //~ WARN: statement has no effect
        return (address(this).balance, count);
    }

    function receiverEffects() external returns (uint256) {
        next().transfer;
        next().send;
        next().call;
        next().delegatecall;
        next().staticcall;
        IToken(next()).transfer;
        next().call{value: 0};
        IToken(next()).transfer{gas: 1};
        IToken(next()).transfer.selector;
        return count;
    }

    function tupleEffects() external returns (uint256) {
        (next().transfer, block);
        (msg, next().send);
        return count;
    }

    function moduleSelector() external pure returns (bytes4) {
        return Module.IModule.f.selector;
    }

    function revertingReceiver() external pure {
        fail().transfer;
    }

    function storageReferences() external view {
        count;
        entries;
        entries[0];
    }

    receive() external payable {
        Withdrawn; //~ WARN: statement has no effect
    }

    function next() internal returns (address payable) {
        count++;
        return payable(address(this));
    }

    function fail() internal pure returns (address payable) {
        revert();
    }
}
