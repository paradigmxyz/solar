// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract C{
event E0(address indexed a,uint indexed b,bytes32 c);
event E1(address indexed a,uint indexed b,bytes32 c);
event E2(address indexed a,uint indexed b,bytes32 c);
event E3(address indexed a,uint indexed b,bytes32 c);
event E4(address indexed a,uint indexed b,bytes32 c);
event E5(address indexed a,uint indexed b,bytes32 c);
event E6(address indexed a,uint indexed b,bytes32 c);
event E7(address indexed a,uint indexed b,bytes32 c);
event E8(address indexed a,uint indexed b,bytes32 c);
event E9(address indexed a,uint indexed b,bytes32 c);
function f()public{
emit E0(msg.sender,0,bytes32(uint(0)));
emit E1(msg.sender,1,bytes32(uint(1)));
emit E2(msg.sender,2,bytes32(uint(2)));
emit E3(msg.sender,3,bytes32(uint(3)));
emit E4(msg.sender,4,bytes32(uint(4)));
emit E5(msg.sender,5,bytes32(uint(5)));
emit E6(msg.sender,6,bytes32(uint(6)));
emit E7(msg.sender,7,bytes32(uint(7)));
emit E8(msg.sender,8,bytes32(uint(8)));
emit E9(msg.sender,9,bytes32(uint(9)));
}
}
