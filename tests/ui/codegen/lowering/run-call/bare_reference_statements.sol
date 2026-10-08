//@ codegen-matrix: standard
//@ run-call: references; value=5 => 5, 0
//@ run-call: receiverEffects => 9
//@ run-call: tupleEffects => 2
//@ run-call: moduleSelector => 0x26121ff0
//@ run-call-fail: revertingReceiver => 0x
//@ run-call-fail: constantStatement => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: constantMember => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: constantTuple => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
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
    uint256 internal constant OVERFLOW = MAX + 1;
    string internal constant NAME = "bare";

    struct Entry {
        uint256 value;
    }

    uint256 private count;
    Entry[] private entries;

    function references() external payable returns (uint256, uint256) {
        address payable to = payable(address(0xdead));
        block;
        msg;
        tx;
        abi;
        Withdrawn;
        Failed;
        IToken;
        Module;
        Module.helper;
        Module.IModule.f;
        Module.IModule.f.selector;
        type(IToken);
        MAX;
        BareReferenceStatements.MAX;
        NAME;
        new uint256[];
        to.transfer;
        to.send;
        to.call;
        to.delegatecall;
        to.staticcall;
        abi.encode;
        abi.decode;
        (block, msg.sender, Withdrawn);
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

    function constantStatement() external pure {
        OVERFLOW;
    }

    function constantMember() external pure {
        BareReferenceStatements.OVERFLOW;
    }

    function constantTuple() external pure {
        (block, OVERFLOW);
    }

    function storageReferences() external view {
        count;
        entries;
        entries[0];
    }

    receive() external payable {
        Withdrawn;
    }

    function next() internal returns (address payable) {
        count++;
        return payable(address(this));
    }

    function fail() internal pure returns (address payable) {
        revert();
    }
}
