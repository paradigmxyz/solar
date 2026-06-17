// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract C{
address public o;
uint public n;
modifier m0(uint v){require(v>0,"m0");_;}
modifier m1(uint v){require(v>1,"m1");_;}
modifier m2(uint v){require(v>2,"m2");_;}
modifier m3(uint v){require(v>3,"m3");_;}
modifier m4(uint v){require(v>4,"m4");_;}
modifier m5(uint v){require(v>5,"m5");_;}
modifier m6(uint v){require(v>6,"m6");_;}
modifier m7(uint v){require(v>7,"m7");_;}
modifier m8(uint v){require(v>8,"m8");_;}
modifier m9(uint v){require(v>9,"m9");_;}
function f(uint x)public m0(x) m1(x) m2(x) m3(x) m4(x) m5(x) m6(x) m7(x) m8(x) m9(x) returns(uint r){r=x;}
}
