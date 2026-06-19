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
contract L10 is L9{
uint public l10;
function g10()public pure returns(uint r){r=10;}
}
contract L11 is L10{
uint public l11;
function g11()public pure returns(uint r){r=11;}
}
contract L12 is L11{
uint public l12;
function g12()public pure returns(uint r){r=12;}
}
contract L13 is L12{
uint public l13;
function g13()public pure returns(uint r){r=13;}
}
contract L14 is L13{
uint public l14;
function g14()public pure returns(uint r){r=14;}
}
contract L15 is L14{
uint public l15;
function g15()public pure returns(uint r){r=15;}
}
contract L16 is L15{
uint public l16;
function g16()public pure returns(uint r){r=16;}
}
contract L17 is L16{
uint public l17;
function g17()public pure returns(uint r){r=17;}
}
contract L18 is L17{
uint public l18;
function g18()public pure returns(uint r){r=18;}
}
contract L19 is L18{
uint public l19;
function g19()public pure returns(uint r){r=19;}
}
contract L20 is L19{
uint public l20;
function g20()public pure returns(uint r){r=20;}
}
contract L21 is L20{
uint public l21;
function g21()public pure returns(uint r){r=21;}
}
contract L22 is L21{
uint public l22;
function g22()public pure returns(uint r){r=22;}
}
contract L23 is L22{
uint public l23;
function g23()public pure returns(uint r){r=23;}
}
contract L24 is L23{
uint public l24;
function g24()public pure returns(uint r){r=24;}
}
contract L25 is L24{
uint public l25;
function g25()public pure returns(uint r){r=25;}
}
contract L26 is L25{
uint public l26;
function g26()public pure returns(uint r){r=26;}
}
contract L27 is L26{
uint public l27;
function g27()public pure returns(uint r){r=27;}
}
contract L28 is L27{
uint public l28;
function g28()public pure returns(uint r){r=28;}
}
contract L29 is L28{
uint public l29;
function g29()public pure returns(uint r){r=29;}
}
contract L30 is L29{
uint public l30;
function g30()public pure returns(uint r){r=30;}
}
contract L31 is L30{
uint public l31;
function g31()public pure returns(uint r){r=31;}
}
contract L32 is L31{
uint public l32;
function g32()public pure returns(uint r){r=32;}
}
contract L33 is L32{
uint public l33;
function g33()public pure returns(uint r){r=33;}
}
contract L34 is L33{
uint public l34;
function g34()public pure returns(uint r){r=34;}
}
contract L35 is L34{
uint public l35;
function g35()public pure returns(uint r){r=35;}
}
contract L36 is L35{
uint public l36;
function g36()public pure returns(uint r){r=36;}
}
contract L37 is L36{
uint public l37;
function g37()public pure returns(uint r){r=37;}
}
contract L38 is L37{
uint public l38;
function g38()public pure returns(uint r){r=38;}
}
contract L39 is L38{
uint public l39;
function g39()public pure returns(uint r){r=39;}
}
contract L40 is L39{
uint public l40;
function g40()public pure returns(uint r){r=40;}
}
contract L41 is L40{
uint public l41;
function g41()public pure returns(uint r){r=41;}
}
contract L42 is L41{
uint public l42;
function g42()public pure returns(uint r){r=42;}
}
contract L43 is L42{
uint public l43;
function g43()public pure returns(uint r){r=43;}
}
contract L44 is L43{
uint public l44;
function g44()public pure returns(uint r){r=44;}
}
contract L45 is L44{
uint public l45;
function g45()public pure returns(uint r){r=45;}
}
contract L46 is L45{
uint public l46;
function g46()public pure returns(uint r){r=46;}
}
contract L47 is L46{
uint public l47;
function g47()public pure returns(uint r){r=47;}
}
contract L48 is L47{
uint public l48;
function g48()public pure returns(uint r){r=48;}
}
contract L49 is L48{
uint public l49;
function g49()public pure returns(uint r){r=49;}
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
contract R10 is R9{
uint public r10;
function h10()public pure returns(uint r){r=10;}
}
contract R11 is R10{
uint public r11;
function h11()public pure returns(uint r){r=11;}
}
contract R12 is R11{
uint public r12;
function h12()public pure returns(uint r){r=12;}
}
contract R13 is R12{
uint public r13;
function h13()public pure returns(uint r){r=13;}
}
contract R14 is R13{
uint public r14;
function h14()public pure returns(uint r){r=14;}
}
contract R15 is R14{
uint public r15;
function h15()public pure returns(uint r){r=15;}
}
contract R16 is R15{
uint public r16;
function h16()public pure returns(uint r){r=16;}
}
contract R17 is R16{
uint public r17;
function h17()public pure returns(uint r){r=17;}
}
contract R18 is R17{
uint public r18;
function h18()public pure returns(uint r){r=18;}
}
contract R19 is R18{
uint public r19;
function h19()public pure returns(uint r){r=19;}
}
contract R20 is R19{
uint public r20;
function h20()public pure returns(uint r){r=20;}
}
contract R21 is R20{
uint public r21;
function h21()public pure returns(uint r){r=21;}
}
contract R22 is R21{
uint public r22;
function h22()public pure returns(uint r){r=22;}
}
contract R23 is R22{
uint public r23;
function h23()public pure returns(uint r){r=23;}
}
contract R24 is R23{
uint public r24;
function h24()public pure returns(uint r){r=24;}
}
contract R25 is R24{
uint public r25;
function h25()public pure returns(uint r){r=25;}
}
contract R26 is R25{
uint public r26;
function h26()public pure returns(uint r){r=26;}
}
contract R27 is R26{
uint public r27;
function h27()public pure returns(uint r){r=27;}
}
contract R28 is R27{
uint public r28;
function h28()public pure returns(uint r){r=28;}
}
contract R29 is R28{
uint public r29;
function h29()public pure returns(uint r){r=29;}
}
contract R30 is R29{
uint public r30;
function h30()public pure returns(uint r){r=30;}
}
contract R31 is R30{
uint public r31;
function h31()public pure returns(uint r){r=31;}
}
contract R32 is R31{
uint public r32;
function h32()public pure returns(uint r){r=32;}
}
contract R33 is R32{
uint public r33;
function h33()public pure returns(uint r){r=33;}
}
contract R34 is R33{
uint public r34;
function h34()public pure returns(uint r){r=34;}
}
contract R35 is R34{
uint public r35;
function h35()public pure returns(uint r){r=35;}
}
contract R36 is R35{
uint public r36;
function h36()public pure returns(uint r){r=36;}
}
contract R37 is R36{
uint public r37;
function h37()public pure returns(uint r){r=37;}
}
contract R38 is R37{
uint public r38;
function h38()public pure returns(uint r){r=38;}
}
contract R39 is R38{
uint public r39;
function h39()public pure returns(uint r){r=39;}
}
contract R40 is R39{
uint public r40;
function h40()public pure returns(uint r){r=40;}
}
contract R41 is R40{
uint public r41;
function h41()public pure returns(uint r){r=41;}
}
contract R42 is R41{
uint public r42;
function h42()public pure returns(uint r){r=42;}
}
contract R43 is R42{
uint public r43;
function h43()public pure returns(uint r){r=43;}
}
contract R44 is R43{
uint public r44;
function h44()public pure returns(uint r){r=44;}
}
contract R45 is R44{
uint public r45;
function h45()public pure returns(uint r){r=45;}
}
contract R46 is R45{
uint public r46;
function h46()public pure returns(uint r){r=46;}
}
contract R47 is R46{
uint public r47;
function h47()public pure returns(uint r){r=47;}
}
contract R48 is R47{
uint public r48;
function h48()public pure returns(uint r){r=48;}
}
contract R49 is R48{
uint public r49;
function h49()public pure returns(uint r){r=49;}
}
contract D is L49,R49{
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
r+=g10()+h10();
r+=g11()+h11();
r+=g12()+h12();
r+=g13()+h13();
r+=g14()+h14();
r+=g15()+h15();
r+=g16()+h16();
r+=g17()+h17();
r+=g18()+h18();
r+=g19()+h19();
}
}
