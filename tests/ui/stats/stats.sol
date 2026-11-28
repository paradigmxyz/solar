//@ revisions: ast hir
//@[ast] compile-flags: -Zast-stats
//@[hir] compile-flags: -Zhir-stats
pragma solidity ^0.8.13;

contract Counter {
    uint256 public number;

    function setNumber(uint256 newNumber) public {
        number = newNumber;
    }

    function increment() public {
        number++;
    }
}
