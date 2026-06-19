// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract B{
uint public v;
function f()public virtual returns(uint r){r=0;}
}
contract L0 is B{
uint public l0;
function g0()public pure returns(uint r){r=0;}
}
contract L1 is L0{
uint public l1;
function g1()public pure returns(uint r){r=1;}
}
contract L2 is L1{
uint public l2;
function g2()public pure returns(uint r){r=2;}
}
contract L3 is L2{
uint public l3;
function g3()public pure returns(uint r){r=3;}
}
contract L4 is L3{
uint public l4;
function g4()public pure returns(uint r){r=4;}
}
contract L5 is L4{
uint public l5;
function g5()public pure returns(uint r){r=5;}
}
contract L6 is L5{
uint public l6;
function g6()public pure returns(uint r){r=6;}
}
contract L7 is L6{
uint public l7;
function g7()public pure returns(uint r){r=7;}
}
contract L8 is L7{
uint public l8;
function g8()public pure returns(uint r){r=8;}
}
contract L9 is L8{
uint public l9;
function g9()public pure returns(uint r){r=9;}
}
contract R0 is B{
uint public r0;
function h0()public pure returns(uint r){r=0;}
}
contract R1 is R0{
uint public r1;
function h1()public pure returns(uint r){r=1;}
}
contract R2 is R1{
uint public r2;
function h2()public pure returns(uint r){r=2;}
}
contract R3 is R2{
uint public r3;
function h3()public pure returns(uint r){r=3;}
}
contract R4 is R3{
uint public r4;
function h4()public pure returns(uint r){r=4;}
}
contract R5 is R4{
uint public r5;
function h5()public pure returns(uint r){r=5;}
}
contract R6 is R5{
uint public r6;
function h6()public pure returns(uint r){r=6;}
}
contract R7 is R6{
uint public r7;
function h7()public pure returns(uint r){r=7;}
}
contract R8 is R7{
uint public r8;
function h8()public pure returns(uint r){r=8;}
}
contract R9 is R8{
uint public r9;
function h9()public pure returns(uint r){r=9;}
}
contract D is L9,R9{
function f()public pure override returns(uint r){r=42;}
function d()public pure returns(uint r){
r+=g0()+h0();
r+=g1()+h1();
r+=g2()+h2();
r+=g3()+h3();
r+=g4()+h4();
r+=g5()+h5();
r+=g6()+h6();
r+=g7()+h7();
r+=g8()+h8();
r+=g9()+h9();
}
}
