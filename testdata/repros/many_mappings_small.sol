// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract C{
mapping(address=>uint) public m0;
mapping(uint=>mapping(address=>uint)) public n1;
mapping(bytes32=>address) public h2;
mapping(address=>mapping(uint=>bool)) public d3;
mapping(address=>uint) public m4;
mapping(uint=>mapping(address=>uint)) public n5;
mapping(bytes32=>address) public h6;
mapping(address=>mapping(uint=>bool)) public d7;
mapping(address=>uint) public m8;
mapping(uint=>mapping(address=>uint)) public n9;
function f(address a,uint v)public{
m0[a]=v;
n1[v][a]=v;
h2[keccak256(abi.encode(v))]=a;
d3[a][v]=true;
m4[a]=v;
n5[v][a]=v;
h6[keccak256(abi.encode(v))]=a;
d7[a][v]=true;
m8[a]=v;
n9[v][a]=v;
}
}
