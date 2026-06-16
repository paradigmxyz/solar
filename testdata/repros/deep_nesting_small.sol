// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract C{
function f(uint x)public pure returns(uint r){r=x;
if(r>0){r=r+1;
if(r>1){r=r+1;
if(r>2){r=r+1;
if(r>3){r=r+1;
if(r>4){r=r+1;
if(r>5){r=r+1;
if(r>6){r=r+1;
if(r>7){r=r+1;
if(r>8){r=r+1;
if(r>9){r=r+1;
}}}}}}}}}}
}
function g(uint n)public pure returns(uint r){
for(uint i0=0;i0<n;i0++){
for(uint i1=0;i1<n;i1++){
for(uint i2=0;i2<n;i2++){
for(uint i3=0;i3<n;i3++){
for(uint i4=0;i4<n;i4++){
for(uint i5=0;i5<n;i5++){
for(uint i6=0;i6<n;i6++){
for(uint i7=0;i7<n;i7++){
for(uint i8=0;i8<n;i8++){
for(uint i9=0;i9<n;i9++){
r+=1;
}}}}}}}}}}
}
}
