//@ revisions: none gas size amsterdam
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@[amsterdam] compile-flags: -O gas --evm-version amsterdam
//@ run-call: Carried18Harness::run => 1
//@ run-call: Carried40Harness::run => 1
//@ run-call: Carried300Harness::run => 1
//@ run-call: CarriedRecursiveHarness::run => 1

// Values loaded from empty storage live across calls to helpers that write memory they never
// allocated, after a low-memory copy. No spill address is safe from the helpers and storage
// loads cannot be recomputed, so the values ride the stack across each call, below `DUP`
// reach when there are too many, and move to the relocated spill area afterwards.
// The standard matrix's `mir` revision would snapshot the MIR of every contract here
// without testing anything the runtime calls do not.
// https://github.com/paradigmxyz/solar/issues/1625

contract Carried18 {
    function append(uint256 n) internal view {
        for (uint256 i = 0; i < n; i++) {
            assembly { mstore(add(0x80, add(calldatasize(), mul(i, 0x20))), caller()) }
        }
    }

    function copy() internal pure {
        assembly { calldatacopy(0, 0, calldatasize()) }
    }

    fallback() external {
        uint256 v0;
        assembly { v0 := add(sload(0), 7) }
        uint256 v1;
        assembly { v1 := add(sload(1), 65544) }
        uint256 v2;
        assembly { v2 := add(sload(2), 131081) }
        uint256 v3;
        assembly { v3 := add(sload(3), 196618) }
        uint256 v4;
        assembly { v4 := add(sload(4), 262155) }
        uint256 v5;
        assembly { v5 := add(sload(5), 327692) }
        uint256 v6;
        assembly { v6 := add(sload(6), 393229) }
        uint256 v7;
        assembly { v7 := add(sload(7), 458766) }
        uint256 v8;
        assembly { v8 := add(sload(8), 524303) }
        uint256 v9;
        assembly { v9 := add(sload(9), 589840) }
        uint256 v10;
        assembly { v10 := add(sload(10), 655377) }
        uint256 v11;
        assembly { v11 := add(sload(11), 720914) }
        uint256 v12;
        assembly { v12 := add(sload(12), 786451) }
        uint256 v13;
        assembly { v13 := add(sload(13), 851988) }
        uint256 v14;
        assembly { v14 := add(sload(14), 917525) }
        uint256 v15;
        assembly { v15 := add(sload(15), 983062) }
        uint256 v16;
        assembly { v16 := add(sload(16), 1048599) }
        uint256 v17;
        assembly { v17 := add(sload(17), 1114136) }
        assembly { calldatacopy(0x80, 0, calldatasize()) }
        append(2);
        copy();
        append(1);
        uint256 r = (v0 << 0)
            ^ (v1 << 1)
            ^ (v2 << 2)
            ^ (v3 << 3)
            ^ (v4 << 4)
            ^ (v5 << 5)
            ^ (v6 << 6)
            ^ (v7 << 7)
            ^ (v8 << 8)
            ^ (v9 << 9)
            ^ (v10 << 10)
            ^ (v11 << 11)
            ^ (v12 << 12)
            ^ (v13 << 13)
            ^ (v14 << 14)
            ^ (v15 << 15)
            ^ (v16 << 16)
            ^ (v17 << 17);
        assembly {
            mstore(0, r)
            return(0, 0x20)
        }
    }
}

contract Carried40 {
    function append(uint256 n) internal view {
        for (uint256 i = 0; i < n; i++) {
            assembly { mstore(add(0x80, add(calldatasize(), mul(i, 0x20))), caller()) }
        }
    }

    function copy() internal pure {
        assembly { calldatacopy(0, 0, calldatasize()) }
    }

    fallback() external {
        uint256 v0;
        assembly { v0 := add(sload(0), 7) }
        uint256 v1;
        assembly { v1 := add(sload(1), 65544) }
        uint256 v2;
        assembly { v2 := add(sload(2), 131081) }
        uint256 v3;
        assembly { v3 := add(sload(3), 196618) }
        uint256 v4;
        assembly { v4 := add(sload(4), 262155) }
        uint256 v5;
        assembly { v5 := add(sload(5), 327692) }
        uint256 v6;
        assembly { v6 := add(sload(6), 393229) }
        uint256 v7;
        assembly { v7 := add(sload(7), 458766) }
        uint256 v8;
        assembly { v8 := add(sload(8), 524303) }
        uint256 v9;
        assembly { v9 := add(sload(9), 589840) }
        uint256 v10;
        assembly { v10 := add(sload(10), 655377) }
        uint256 v11;
        assembly { v11 := add(sload(11), 720914) }
        uint256 v12;
        assembly { v12 := add(sload(12), 786451) }
        uint256 v13;
        assembly { v13 := add(sload(13), 851988) }
        uint256 v14;
        assembly { v14 := add(sload(14), 917525) }
        uint256 v15;
        assembly { v15 := add(sload(15), 983062) }
        uint256 v16;
        assembly { v16 := add(sload(16), 1048599) }
        uint256 v17;
        assembly { v17 := add(sload(17), 1114136) }
        uint256 v18;
        assembly { v18 := add(sload(18), 1179673) }
        uint256 v19;
        assembly { v19 := add(sload(19), 1245210) }
        uint256 v20;
        assembly { v20 := add(sload(20), 1310747) }
        uint256 v21;
        assembly { v21 := add(sload(21), 1376284) }
        uint256 v22;
        assembly { v22 := add(sload(22), 1441821) }
        uint256 v23;
        assembly { v23 := add(sload(23), 1507358) }
        uint256 v24;
        assembly { v24 := add(sload(24), 1572895) }
        uint256 v25;
        assembly { v25 := add(sload(25), 1638432) }
        uint256 v26;
        assembly { v26 := add(sload(26), 1703969) }
        uint256 v27;
        assembly { v27 := add(sload(27), 1769506) }
        uint256 v28;
        assembly { v28 := add(sload(28), 1835043) }
        uint256 v29;
        assembly { v29 := add(sload(29), 1900580) }
        uint256 v30;
        assembly { v30 := add(sload(30), 1966117) }
        uint256 v31;
        assembly { v31 := add(sload(31), 2031654) }
        uint256 v32;
        assembly { v32 := add(sload(32), 2097191) }
        uint256 v33;
        assembly { v33 := add(sload(33), 2162728) }
        uint256 v34;
        assembly { v34 := add(sload(34), 2228265) }
        uint256 v35;
        assembly { v35 := add(sload(35), 2293802) }
        uint256 v36;
        assembly { v36 := add(sload(36), 2359339) }
        uint256 v37;
        assembly { v37 := add(sload(37), 2424876) }
        uint256 v38;
        assembly { v38 := add(sload(38), 2490413) }
        uint256 v39;
        assembly { v39 := add(sload(39), 2555950) }
        assembly { calldatacopy(0x80, 0, calldatasize()) }
        append(2);
        copy();
        append(1);
        uint256 r = (v0 << 0)
            ^ (v1 << 1)
            ^ (v2 << 2)
            ^ (v3 << 3)
            ^ (v4 << 4)
            ^ (v5 << 5)
            ^ (v6 << 6)
            ^ (v7 << 7)
            ^ (v8 << 8)
            ^ (v9 << 9)
            ^ (v10 << 10)
            ^ (v11 << 11)
            ^ (v12 << 12)
            ^ (v13 << 13)
            ^ (v14 << 14)
            ^ (v15 << 15)
            ^ (v16 << 16)
            ^ (v17 << 17)
            ^ (v18 << 18)
            ^ (v19 << 19)
            ^ (v20 << 20)
            ^ (v21 << 21)
            ^ (v22 << 22)
            ^ (v23 << 23)
            ^ (v24 << 24)
            ^ (v25 << 25)
            ^ (v26 << 26)
            ^ (v27 << 27)
            ^ (v28 << 28)
            ^ (v29 << 29)
            ^ (v30 << 30)
            ^ (v31 << 31)
            ^ (v32 << 32)
            ^ (v33 << 33)
            ^ (v34 << 34)
            ^ (v35 << 35)
            ^ (v36 << 36)
            ^ (v37 << 37)
            ^ (v38 << 38)
            ^ (v39 << 39);
        assembly {
            mstore(0, r)
            return(0, 0x20)
        }
    }
}

contract Carried300 {
    function append(uint256 n) internal view {
        for (uint256 i = 0; i < n; i++) {
            assembly { mstore(add(0x80, add(calldatasize(), mul(i, 0x20))), caller()) }
        }
    }

    function copy() internal pure {
        assembly { calldatacopy(0, 0, calldatasize()) }
    }

    fallback() external {
        uint256 v0;
        assembly { v0 := add(sload(0), 7) }
        uint256 v1;
        assembly { v1 := add(sload(1), 65544) }
        uint256 v2;
        assembly { v2 := add(sload(2), 131081) }
        uint256 v3;
        assembly { v3 := add(sload(3), 196618) }
        uint256 v4;
        assembly { v4 := add(sload(4), 262155) }
        uint256 v5;
        assembly { v5 := add(sload(5), 327692) }
        uint256 v6;
        assembly { v6 := add(sload(6), 393229) }
        uint256 v7;
        assembly { v7 := add(sload(7), 458766) }
        uint256 v8;
        assembly { v8 := add(sload(8), 524303) }
        uint256 v9;
        assembly { v9 := add(sload(9), 589840) }
        uint256 v10;
        assembly { v10 := add(sload(10), 655377) }
        uint256 v11;
        assembly { v11 := add(sload(11), 720914) }
        uint256 v12;
        assembly { v12 := add(sload(12), 786451) }
        uint256 v13;
        assembly { v13 := add(sload(13), 851988) }
        uint256 v14;
        assembly { v14 := add(sload(14), 917525) }
        uint256 v15;
        assembly { v15 := add(sload(15), 983062) }
        uint256 v16;
        assembly { v16 := add(sload(16), 1048599) }
        uint256 v17;
        assembly { v17 := add(sload(17), 1114136) }
        uint256 v18;
        assembly { v18 := add(sload(18), 1179673) }
        uint256 v19;
        assembly { v19 := add(sload(19), 1245210) }
        uint256 v20;
        assembly { v20 := add(sload(20), 1310747) }
        uint256 v21;
        assembly { v21 := add(sload(21), 1376284) }
        uint256 v22;
        assembly { v22 := add(sload(22), 1441821) }
        uint256 v23;
        assembly { v23 := add(sload(23), 1507358) }
        uint256 v24;
        assembly { v24 := add(sload(24), 1572895) }
        uint256 v25;
        assembly { v25 := add(sload(25), 1638432) }
        uint256 v26;
        assembly { v26 := add(sload(26), 1703969) }
        uint256 v27;
        assembly { v27 := add(sload(27), 1769506) }
        uint256 v28;
        assembly { v28 := add(sload(28), 1835043) }
        uint256 v29;
        assembly { v29 := add(sload(29), 1900580) }
        uint256 v30;
        assembly { v30 := add(sload(30), 1966117) }
        uint256 v31;
        assembly { v31 := add(sload(31), 2031654) }
        uint256 v32;
        assembly { v32 := add(sload(32), 2097191) }
        uint256 v33;
        assembly { v33 := add(sload(33), 2162728) }
        uint256 v34;
        assembly { v34 := add(sload(34), 2228265) }
        uint256 v35;
        assembly { v35 := add(sload(35), 2293802) }
        uint256 v36;
        assembly { v36 := add(sload(36), 2359339) }
        uint256 v37;
        assembly { v37 := add(sload(37), 2424876) }
        uint256 v38;
        assembly { v38 := add(sload(38), 2490413) }
        uint256 v39;
        assembly { v39 := add(sload(39), 2555950) }
        uint256 v40;
        assembly { v40 := add(sload(40), 2621487) }
        uint256 v41;
        assembly { v41 := add(sload(41), 2687024) }
        uint256 v42;
        assembly { v42 := add(sload(42), 2752561) }
        uint256 v43;
        assembly { v43 := add(sload(43), 2818098) }
        uint256 v44;
        assembly { v44 := add(sload(44), 2883635) }
        uint256 v45;
        assembly { v45 := add(sload(45), 2949172) }
        uint256 v46;
        assembly { v46 := add(sload(46), 3014709) }
        uint256 v47;
        assembly { v47 := add(sload(47), 3080246) }
        uint256 v48;
        assembly { v48 := add(sload(48), 3145783) }
        uint256 v49;
        assembly { v49 := add(sload(49), 3211320) }
        uint256 v50;
        assembly { v50 := add(sload(50), 3276857) }
        uint256 v51;
        assembly { v51 := add(sload(51), 3342394) }
        uint256 v52;
        assembly { v52 := add(sload(52), 3407931) }
        uint256 v53;
        assembly { v53 := add(sload(53), 3473468) }
        uint256 v54;
        assembly { v54 := add(sload(54), 3539005) }
        uint256 v55;
        assembly { v55 := add(sload(55), 3604542) }
        uint256 v56;
        assembly { v56 := add(sload(56), 3670079) }
        uint256 v57;
        assembly { v57 := add(sload(57), 3735616) }
        uint256 v58;
        assembly { v58 := add(sload(58), 3801153) }
        uint256 v59;
        assembly { v59 := add(sload(59), 3866690) }
        uint256 v60;
        assembly { v60 := add(sload(60), 3932227) }
        uint256 v61;
        assembly { v61 := add(sload(61), 3997764) }
        uint256 v62;
        assembly { v62 := add(sload(62), 4063301) }
        uint256 v63;
        assembly { v63 := add(sload(63), 4128838) }
        uint256 v64;
        assembly { v64 := add(sload(64), 4194375) }
        uint256 v65;
        assembly { v65 := add(sload(65), 4259912) }
        uint256 v66;
        assembly { v66 := add(sload(66), 4325449) }
        uint256 v67;
        assembly { v67 := add(sload(67), 4390986) }
        uint256 v68;
        assembly { v68 := add(sload(68), 4456523) }
        uint256 v69;
        assembly { v69 := add(sload(69), 4522060) }
        uint256 v70;
        assembly { v70 := add(sload(70), 4587597) }
        uint256 v71;
        assembly { v71 := add(sload(71), 4653134) }
        uint256 v72;
        assembly { v72 := add(sload(72), 4718671) }
        uint256 v73;
        assembly { v73 := add(sload(73), 4784208) }
        uint256 v74;
        assembly { v74 := add(sload(74), 4849745) }
        uint256 v75;
        assembly { v75 := add(sload(75), 4915282) }
        uint256 v76;
        assembly { v76 := add(sload(76), 4980819) }
        uint256 v77;
        assembly { v77 := add(sload(77), 5046356) }
        uint256 v78;
        assembly { v78 := add(sload(78), 5111893) }
        uint256 v79;
        assembly { v79 := add(sload(79), 5177430) }
        uint256 v80;
        assembly { v80 := add(sload(80), 5242967) }
        uint256 v81;
        assembly { v81 := add(sload(81), 5308504) }
        uint256 v82;
        assembly { v82 := add(sload(82), 5374041) }
        uint256 v83;
        assembly { v83 := add(sload(83), 5439578) }
        uint256 v84;
        assembly { v84 := add(sload(84), 5505115) }
        uint256 v85;
        assembly { v85 := add(sload(85), 5570652) }
        uint256 v86;
        assembly { v86 := add(sload(86), 5636189) }
        uint256 v87;
        assembly { v87 := add(sload(87), 5701726) }
        uint256 v88;
        assembly { v88 := add(sload(88), 5767263) }
        uint256 v89;
        assembly { v89 := add(sload(89), 5832800) }
        uint256 v90;
        assembly { v90 := add(sload(90), 5898337) }
        uint256 v91;
        assembly { v91 := add(sload(91), 5963874) }
        uint256 v92;
        assembly { v92 := add(sload(92), 6029411) }
        uint256 v93;
        assembly { v93 := add(sload(93), 6094948) }
        uint256 v94;
        assembly { v94 := add(sload(94), 6160485) }
        uint256 v95;
        assembly { v95 := add(sload(95), 6226022) }
        uint256 v96;
        assembly { v96 := add(sload(96), 6291559) }
        uint256 v97;
        assembly { v97 := add(sload(97), 6357096) }
        uint256 v98;
        assembly { v98 := add(sload(98), 6422633) }
        uint256 v99;
        assembly { v99 := add(sload(99), 6488170) }
        uint256 v100;
        assembly { v100 := add(sload(100), 6553707) }
        uint256 v101;
        assembly { v101 := add(sload(101), 6619244) }
        uint256 v102;
        assembly { v102 := add(sload(102), 6684781) }
        uint256 v103;
        assembly { v103 := add(sload(103), 6750318) }
        uint256 v104;
        assembly { v104 := add(sload(104), 6815855) }
        uint256 v105;
        assembly { v105 := add(sload(105), 6881392) }
        uint256 v106;
        assembly { v106 := add(sload(106), 6946929) }
        uint256 v107;
        assembly { v107 := add(sload(107), 7012466) }
        uint256 v108;
        assembly { v108 := add(sload(108), 7078003) }
        uint256 v109;
        assembly { v109 := add(sload(109), 7143540) }
        uint256 v110;
        assembly { v110 := add(sload(110), 7209077) }
        uint256 v111;
        assembly { v111 := add(sload(111), 7274614) }
        uint256 v112;
        assembly { v112 := add(sload(112), 7340151) }
        uint256 v113;
        assembly { v113 := add(sload(113), 7405688) }
        uint256 v114;
        assembly { v114 := add(sload(114), 7471225) }
        uint256 v115;
        assembly { v115 := add(sload(115), 7536762) }
        uint256 v116;
        assembly { v116 := add(sload(116), 7602299) }
        uint256 v117;
        assembly { v117 := add(sload(117), 7667836) }
        uint256 v118;
        assembly { v118 := add(sload(118), 7733373) }
        uint256 v119;
        assembly { v119 := add(sload(119), 7798910) }
        uint256 v120;
        assembly { v120 := add(sload(120), 7864447) }
        uint256 v121;
        assembly { v121 := add(sload(121), 7929984) }
        uint256 v122;
        assembly { v122 := add(sload(122), 7995521) }
        uint256 v123;
        assembly { v123 := add(sload(123), 8061058) }
        uint256 v124;
        assembly { v124 := add(sload(124), 8126595) }
        uint256 v125;
        assembly { v125 := add(sload(125), 8192132) }
        uint256 v126;
        assembly { v126 := add(sload(126), 8257669) }
        uint256 v127;
        assembly { v127 := add(sload(127), 8323206) }
        uint256 v128;
        assembly { v128 := add(sload(128), 8388743) }
        uint256 v129;
        assembly { v129 := add(sload(129), 8454280) }
        uint256 v130;
        assembly { v130 := add(sload(130), 8519817) }
        uint256 v131;
        assembly { v131 := add(sload(131), 8585354) }
        uint256 v132;
        assembly { v132 := add(sload(132), 8650891) }
        uint256 v133;
        assembly { v133 := add(sload(133), 8716428) }
        uint256 v134;
        assembly { v134 := add(sload(134), 8781965) }
        uint256 v135;
        assembly { v135 := add(sload(135), 8847502) }
        uint256 v136;
        assembly { v136 := add(sload(136), 8913039) }
        uint256 v137;
        assembly { v137 := add(sload(137), 8978576) }
        uint256 v138;
        assembly { v138 := add(sload(138), 9044113) }
        uint256 v139;
        assembly { v139 := add(sload(139), 9109650) }
        uint256 v140;
        assembly { v140 := add(sload(140), 9175187) }
        uint256 v141;
        assembly { v141 := add(sload(141), 9240724) }
        uint256 v142;
        assembly { v142 := add(sload(142), 9306261) }
        uint256 v143;
        assembly { v143 := add(sload(143), 9371798) }
        uint256 v144;
        assembly { v144 := add(sload(144), 9437335) }
        uint256 v145;
        assembly { v145 := add(sload(145), 9502872) }
        uint256 v146;
        assembly { v146 := add(sload(146), 9568409) }
        uint256 v147;
        assembly { v147 := add(sload(147), 9633946) }
        uint256 v148;
        assembly { v148 := add(sload(148), 9699483) }
        uint256 v149;
        assembly { v149 := add(sload(149), 9765020) }
        uint256 v150;
        assembly { v150 := add(sload(150), 9830557) }
        uint256 v151;
        assembly { v151 := add(sload(151), 9896094) }
        uint256 v152;
        assembly { v152 := add(sload(152), 9961631) }
        uint256 v153;
        assembly { v153 := add(sload(153), 10027168) }
        uint256 v154;
        assembly { v154 := add(sload(154), 10092705) }
        uint256 v155;
        assembly { v155 := add(sload(155), 10158242) }
        uint256 v156;
        assembly { v156 := add(sload(156), 10223779) }
        uint256 v157;
        assembly { v157 := add(sload(157), 10289316) }
        uint256 v158;
        assembly { v158 := add(sload(158), 10354853) }
        uint256 v159;
        assembly { v159 := add(sload(159), 10420390) }
        uint256 v160;
        assembly { v160 := add(sload(160), 10485927) }
        uint256 v161;
        assembly { v161 := add(sload(161), 10551464) }
        uint256 v162;
        assembly { v162 := add(sload(162), 10617001) }
        uint256 v163;
        assembly { v163 := add(sload(163), 10682538) }
        uint256 v164;
        assembly { v164 := add(sload(164), 10748075) }
        uint256 v165;
        assembly { v165 := add(sload(165), 10813612) }
        uint256 v166;
        assembly { v166 := add(sload(166), 10879149) }
        uint256 v167;
        assembly { v167 := add(sload(167), 10944686) }
        uint256 v168;
        assembly { v168 := add(sload(168), 11010223) }
        uint256 v169;
        assembly { v169 := add(sload(169), 11075760) }
        uint256 v170;
        assembly { v170 := add(sload(170), 11141297) }
        uint256 v171;
        assembly { v171 := add(sload(171), 11206834) }
        uint256 v172;
        assembly { v172 := add(sload(172), 11272371) }
        uint256 v173;
        assembly { v173 := add(sload(173), 11337908) }
        uint256 v174;
        assembly { v174 := add(sload(174), 11403445) }
        uint256 v175;
        assembly { v175 := add(sload(175), 11468982) }
        uint256 v176;
        assembly { v176 := add(sload(176), 11534519) }
        uint256 v177;
        assembly { v177 := add(sload(177), 11600056) }
        uint256 v178;
        assembly { v178 := add(sload(178), 11665593) }
        uint256 v179;
        assembly { v179 := add(sload(179), 11731130) }
        uint256 v180;
        assembly { v180 := add(sload(180), 11796667) }
        uint256 v181;
        assembly { v181 := add(sload(181), 11862204) }
        uint256 v182;
        assembly { v182 := add(sload(182), 11927741) }
        uint256 v183;
        assembly { v183 := add(sload(183), 11993278) }
        uint256 v184;
        assembly { v184 := add(sload(184), 12058815) }
        uint256 v185;
        assembly { v185 := add(sload(185), 12124352) }
        uint256 v186;
        assembly { v186 := add(sload(186), 12189889) }
        uint256 v187;
        assembly { v187 := add(sload(187), 12255426) }
        uint256 v188;
        assembly { v188 := add(sload(188), 12320963) }
        uint256 v189;
        assembly { v189 := add(sload(189), 12386500) }
        uint256 v190;
        assembly { v190 := add(sload(190), 12452037) }
        uint256 v191;
        assembly { v191 := add(sload(191), 12517574) }
        uint256 v192;
        assembly { v192 := add(sload(192), 12583111) }
        uint256 v193;
        assembly { v193 := add(sload(193), 12648648) }
        uint256 v194;
        assembly { v194 := add(sload(194), 12714185) }
        uint256 v195;
        assembly { v195 := add(sload(195), 12779722) }
        uint256 v196;
        assembly { v196 := add(sload(196), 12845259) }
        uint256 v197;
        assembly { v197 := add(sload(197), 12910796) }
        uint256 v198;
        assembly { v198 := add(sload(198), 12976333) }
        uint256 v199;
        assembly { v199 := add(sload(199), 13041870) }
        uint256 v200;
        assembly { v200 := add(sload(200), 13107407) }
        uint256 v201;
        assembly { v201 := add(sload(201), 13172944) }
        uint256 v202;
        assembly { v202 := add(sload(202), 13238481) }
        uint256 v203;
        assembly { v203 := add(sload(203), 13304018) }
        uint256 v204;
        assembly { v204 := add(sload(204), 13369555) }
        uint256 v205;
        assembly { v205 := add(sload(205), 13435092) }
        uint256 v206;
        assembly { v206 := add(sload(206), 13500629) }
        uint256 v207;
        assembly { v207 := add(sload(207), 13566166) }
        uint256 v208;
        assembly { v208 := add(sload(208), 13631703) }
        uint256 v209;
        assembly { v209 := add(sload(209), 13697240) }
        uint256 v210;
        assembly { v210 := add(sload(210), 13762777) }
        uint256 v211;
        assembly { v211 := add(sload(211), 13828314) }
        uint256 v212;
        assembly { v212 := add(sload(212), 13893851) }
        uint256 v213;
        assembly { v213 := add(sload(213), 13959388) }
        uint256 v214;
        assembly { v214 := add(sload(214), 14024925) }
        uint256 v215;
        assembly { v215 := add(sload(215), 14090462) }
        uint256 v216;
        assembly { v216 := add(sload(216), 14155999) }
        uint256 v217;
        assembly { v217 := add(sload(217), 14221536) }
        uint256 v218;
        assembly { v218 := add(sload(218), 14287073) }
        uint256 v219;
        assembly { v219 := add(sload(219), 14352610) }
        uint256 v220;
        assembly { v220 := add(sload(220), 14418147) }
        uint256 v221;
        assembly { v221 := add(sload(221), 14483684) }
        uint256 v222;
        assembly { v222 := add(sload(222), 14549221) }
        uint256 v223;
        assembly { v223 := add(sload(223), 14614758) }
        uint256 v224;
        assembly { v224 := add(sload(224), 14680295) }
        uint256 v225;
        assembly { v225 := add(sload(225), 14745832) }
        uint256 v226;
        assembly { v226 := add(sload(226), 14811369) }
        uint256 v227;
        assembly { v227 := add(sload(227), 14876906) }
        uint256 v228;
        assembly { v228 := add(sload(228), 14942443) }
        uint256 v229;
        assembly { v229 := add(sload(229), 15007980) }
        uint256 v230;
        assembly { v230 := add(sload(230), 15073517) }
        uint256 v231;
        assembly { v231 := add(sload(231), 15139054) }
        uint256 v232;
        assembly { v232 := add(sload(232), 15204591) }
        uint256 v233;
        assembly { v233 := add(sload(233), 15270128) }
        uint256 v234;
        assembly { v234 := add(sload(234), 15335665) }
        uint256 v235;
        assembly { v235 := add(sload(235), 15401202) }
        uint256 v236;
        assembly { v236 := add(sload(236), 15466739) }
        uint256 v237;
        assembly { v237 := add(sload(237), 15532276) }
        uint256 v238;
        assembly { v238 := add(sload(238), 15597813) }
        uint256 v239;
        assembly { v239 := add(sload(239), 15663350) }
        uint256 v240;
        assembly { v240 := add(sload(240), 15728887) }
        uint256 v241;
        assembly { v241 := add(sload(241), 15794424) }
        uint256 v242;
        assembly { v242 := add(sload(242), 15859961) }
        uint256 v243;
        assembly { v243 := add(sload(243), 15925498) }
        uint256 v244;
        assembly { v244 := add(sload(244), 15991035) }
        uint256 v245;
        assembly { v245 := add(sload(245), 16056572) }
        uint256 v246;
        assembly { v246 := add(sload(246), 16122109) }
        uint256 v247;
        assembly { v247 := add(sload(247), 16187646) }
        uint256 v248;
        assembly { v248 := add(sload(248), 16253183) }
        uint256 v249;
        assembly { v249 := add(sload(249), 16318720) }
        uint256 v250;
        assembly { v250 := add(sload(250), 16384257) }
        uint256 v251;
        assembly { v251 := add(sload(251), 16449794) }
        uint256 v252;
        assembly { v252 := add(sload(252), 16515331) }
        uint256 v253;
        assembly { v253 := add(sload(253), 16580868) }
        uint256 v254;
        assembly { v254 := add(sload(254), 16646405) }
        uint256 v255;
        assembly { v255 := add(sload(255), 16711942) }
        uint256 v256;
        assembly { v256 := add(sload(256), 16777479) }
        uint256 v257;
        assembly { v257 := add(sload(257), 16843016) }
        uint256 v258;
        assembly { v258 := add(sload(258), 16908553) }
        uint256 v259;
        assembly { v259 := add(sload(259), 16974090) }
        uint256 v260;
        assembly { v260 := add(sload(260), 17039627) }
        uint256 v261;
        assembly { v261 := add(sload(261), 17105164) }
        uint256 v262;
        assembly { v262 := add(sload(262), 17170701) }
        uint256 v263;
        assembly { v263 := add(sload(263), 17236238) }
        uint256 v264;
        assembly { v264 := add(sload(264), 17301775) }
        uint256 v265;
        assembly { v265 := add(sload(265), 17367312) }
        uint256 v266;
        assembly { v266 := add(sload(266), 17432849) }
        uint256 v267;
        assembly { v267 := add(sload(267), 17498386) }
        uint256 v268;
        assembly { v268 := add(sload(268), 17563923) }
        uint256 v269;
        assembly { v269 := add(sload(269), 17629460) }
        uint256 v270;
        assembly { v270 := add(sload(270), 17694997) }
        uint256 v271;
        assembly { v271 := add(sload(271), 17760534) }
        uint256 v272;
        assembly { v272 := add(sload(272), 17826071) }
        uint256 v273;
        assembly { v273 := add(sload(273), 17891608) }
        uint256 v274;
        assembly { v274 := add(sload(274), 17957145) }
        uint256 v275;
        assembly { v275 := add(sload(275), 18022682) }
        uint256 v276;
        assembly { v276 := add(sload(276), 18088219) }
        uint256 v277;
        assembly { v277 := add(sload(277), 18153756) }
        uint256 v278;
        assembly { v278 := add(sload(278), 18219293) }
        uint256 v279;
        assembly { v279 := add(sload(279), 18284830) }
        uint256 v280;
        assembly { v280 := add(sload(280), 18350367) }
        uint256 v281;
        assembly { v281 := add(sload(281), 18415904) }
        uint256 v282;
        assembly { v282 := add(sload(282), 18481441) }
        uint256 v283;
        assembly { v283 := add(sload(283), 18546978) }
        uint256 v284;
        assembly { v284 := add(sload(284), 18612515) }
        uint256 v285;
        assembly { v285 := add(sload(285), 18678052) }
        uint256 v286;
        assembly { v286 := add(sload(286), 18743589) }
        uint256 v287;
        assembly { v287 := add(sload(287), 18809126) }
        uint256 v288;
        assembly { v288 := add(sload(288), 18874663) }
        uint256 v289;
        assembly { v289 := add(sload(289), 18940200) }
        uint256 v290;
        assembly { v290 := add(sload(290), 19005737) }
        uint256 v291;
        assembly { v291 := add(sload(291), 19071274) }
        uint256 v292;
        assembly { v292 := add(sload(292), 19136811) }
        uint256 v293;
        assembly { v293 := add(sload(293), 19202348) }
        uint256 v294;
        assembly { v294 := add(sload(294), 19267885) }
        uint256 v295;
        assembly { v295 := add(sload(295), 19333422) }
        uint256 v296;
        assembly { v296 := add(sload(296), 19398959) }
        uint256 v297;
        assembly { v297 := add(sload(297), 19464496) }
        uint256 v298;
        assembly { v298 := add(sload(298), 19530033) }
        uint256 v299;
        assembly { v299 := add(sload(299), 19595570) }
        assembly { calldatacopy(0x80, 0, calldatasize()) }
        append(2);
        copy();
        append(1);
        uint256 r = (v0 << 0)
            ^ (v1 << 1)
            ^ (v2 << 2)
            ^ (v3 << 3)
            ^ (v4 << 4)
            ^ (v5 << 5)
            ^ (v6 << 6)
            ^ (v7 << 7)
            ^ (v8 << 8)
            ^ (v9 << 9)
            ^ (v10 << 10)
            ^ (v11 << 11)
            ^ (v12 << 12)
            ^ (v13 << 13)
            ^ (v14 << 14)
            ^ (v15 << 15)
            ^ (v16 << 16)
            ^ (v17 << 17)
            ^ (v18 << 18)
            ^ (v19 << 19)
            ^ (v20 << 20)
            ^ (v21 << 21)
            ^ (v22 << 22)
            ^ (v23 << 23)
            ^ (v24 << 24)
            ^ (v25 << 25)
            ^ (v26 << 26)
            ^ (v27 << 27)
            ^ (v28 << 28)
            ^ (v29 << 29)
            ^ (v30 << 30)
            ^ (v31 << 31)
            ^ (v32 << 32)
            ^ (v33 << 33)
            ^ (v34 << 34)
            ^ (v35 << 35)
            ^ (v36 << 36)
            ^ (v37 << 37)
            ^ (v38 << 38)
            ^ (v39 << 39)
            ^ (v40 << 40)
            ^ (v41 << 41)
            ^ (v42 << 42)
            ^ (v43 << 43)
            ^ (v44 << 44)
            ^ (v45 << 45)
            ^ (v46 << 46)
            ^ (v47 << 47)
            ^ (v48 << 48)
            ^ (v49 << 49)
            ^ (v50 << 50)
            ^ (v51 << 51)
            ^ (v52 << 52)
            ^ (v53 << 53)
            ^ (v54 << 54)
            ^ (v55 << 55)
            ^ (v56 << 56)
            ^ (v57 << 57)
            ^ (v58 << 58)
            ^ (v59 << 59)
            ^ (v60 << 60)
            ^ (v61 << 61)
            ^ (v62 << 62)
            ^ (v63 << 63)
            ^ (v64 << 64)
            ^ (v65 << 65)
            ^ (v66 << 66)
            ^ (v67 << 67)
            ^ (v68 << 68)
            ^ (v69 << 69)
            ^ (v70 << 70)
            ^ (v71 << 71)
            ^ (v72 << 72)
            ^ (v73 << 73)
            ^ (v74 << 74)
            ^ (v75 << 75)
            ^ (v76 << 76)
            ^ (v77 << 77)
            ^ (v78 << 78)
            ^ (v79 << 79)
            ^ (v80 << 80)
            ^ (v81 << 81)
            ^ (v82 << 82)
            ^ (v83 << 83)
            ^ (v84 << 84)
            ^ (v85 << 85)
            ^ (v86 << 86)
            ^ (v87 << 87)
            ^ (v88 << 88)
            ^ (v89 << 89)
            ^ (v90 << 90)
            ^ (v91 << 91)
            ^ (v92 << 92)
            ^ (v93 << 93)
            ^ (v94 << 94)
            ^ (v95 << 95)
            ^ (v96 << 96)
            ^ (v97 << 97)
            ^ (v98 << 98)
            ^ (v99 << 99)
            ^ (v100 << 100)
            ^ (v101 << 101)
            ^ (v102 << 102)
            ^ (v103 << 103)
            ^ (v104 << 104)
            ^ (v105 << 105)
            ^ (v106 << 106)
            ^ (v107 << 107)
            ^ (v108 << 108)
            ^ (v109 << 109)
            ^ (v110 << 110)
            ^ (v111 << 111)
            ^ (v112 << 112)
            ^ (v113 << 113)
            ^ (v114 << 114)
            ^ (v115 << 115)
            ^ (v116 << 116)
            ^ (v117 << 117)
            ^ (v118 << 118)
            ^ (v119 << 119)
            ^ (v120 << 120)
            ^ (v121 << 121)
            ^ (v122 << 122)
            ^ (v123 << 123)
            ^ (v124 << 124)
            ^ (v125 << 125)
            ^ (v126 << 126)
            ^ (v127 << 127)
            ^ (v128 << 128)
            ^ (v129 << 129)
            ^ (v130 << 130)
            ^ (v131 << 131)
            ^ (v132 << 132)
            ^ (v133 << 133)
            ^ (v134 << 134)
            ^ (v135 << 135)
            ^ (v136 << 136)
            ^ (v137 << 137)
            ^ (v138 << 138)
            ^ (v139 << 139)
            ^ (v140 << 140)
            ^ (v141 << 141)
            ^ (v142 << 142)
            ^ (v143 << 143)
            ^ (v144 << 144)
            ^ (v145 << 145)
            ^ (v146 << 146)
            ^ (v147 << 147)
            ^ (v148 << 148)
            ^ (v149 << 149)
            ^ (v150 << 150)
            ^ (v151 << 151)
            ^ (v152 << 152)
            ^ (v153 << 153)
            ^ (v154 << 154)
            ^ (v155 << 155)
            ^ (v156 << 156)
            ^ (v157 << 157)
            ^ (v158 << 158)
            ^ (v159 << 159)
            ^ (v160 << 160)
            ^ (v161 << 161)
            ^ (v162 << 162)
            ^ (v163 << 163)
            ^ (v164 << 164)
            ^ (v165 << 165)
            ^ (v166 << 166)
            ^ (v167 << 167)
            ^ (v168 << 168)
            ^ (v169 << 169)
            ^ (v170 << 170)
            ^ (v171 << 171)
            ^ (v172 << 172)
            ^ (v173 << 173)
            ^ (v174 << 174)
            ^ (v175 << 175)
            ^ (v176 << 176)
            ^ (v177 << 177)
            ^ (v178 << 178)
            ^ (v179 << 179)
            ^ (v180 << 180)
            ^ (v181 << 181)
            ^ (v182 << 182)
            ^ (v183 << 183)
            ^ (v184 << 184)
            ^ (v185 << 185)
            ^ (v186 << 186)
            ^ (v187 << 187)
            ^ (v188 << 188)
            ^ (v189 << 189)
            ^ (v190 << 190)
            ^ (v191 << 191)
            ^ (v192 << 192)
            ^ (v193 << 193)
            ^ (v194 << 194)
            ^ (v195 << 195)
            ^ (v196 << 196)
            ^ (v197 << 197)
            ^ (v198 << 198)
            ^ (v199 << 199)
            ^ (v200 << 0)
            ^ (v201 << 1)
            ^ (v202 << 2)
            ^ (v203 << 3)
            ^ (v204 << 4)
            ^ (v205 << 5)
            ^ (v206 << 6)
            ^ (v207 << 7)
            ^ (v208 << 8)
            ^ (v209 << 9)
            ^ (v210 << 10)
            ^ (v211 << 11)
            ^ (v212 << 12)
            ^ (v213 << 13)
            ^ (v214 << 14)
            ^ (v215 << 15)
            ^ (v216 << 16)
            ^ (v217 << 17)
            ^ (v218 << 18)
            ^ (v219 << 19)
            ^ (v220 << 20)
            ^ (v221 << 21)
            ^ (v222 << 22)
            ^ (v223 << 23)
            ^ (v224 << 24)
            ^ (v225 << 25)
            ^ (v226 << 26)
            ^ (v227 << 27)
            ^ (v228 << 28)
            ^ (v229 << 29)
            ^ (v230 << 30)
            ^ (v231 << 31)
            ^ (v232 << 32)
            ^ (v233 << 33)
            ^ (v234 << 34)
            ^ (v235 << 35)
            ^ (v236 << 36)
            ^ (v237 << 37)
            ^ (v238 << 38)
            ^ (v239 << 39)
            ^ (v240 << 40)
            ^ (v241 << 41)
            ^ (v242 << 42)
            ^ (v243 << 43)
            ^ (v244 << 44)
            ^ (v245 << 45)
            ^ (v246 << 46)
            ^ (v247 << 47)
            ^ (v248 << 48)
            ^ (v249 << 49)
            ^ (v250 << 50)
            ^ (v251 << 51)
            ^ (v252 << 52)
            ^ (v253 << 53)
            ^ (v254 << 54)
            ^ (v255 << 55)
            ^ (v256 << 56)
            ^ (v257 << 57)
            ^ (v258 << 58)
            ^ (v259 << 59)
            ^ (v260 << 60)
            ^ (v261 << 61)
            ^ (v262 << 62)
            ^ (v263 << 63)
            ^ (v264 << 64)
            ^ (v265 << 65)
            ^ (v266 << 66)
            ^ (v267 << 67)
            ^ (v268 << 68)
            ^ (v269 << 69)
            ^ (v270 << 70)
            ^ (v271 << 71)
            ^ (v272 << 72)
            ^ (v273 << 73)
            ^ (v274 << 74)
            ^ (v275 << 75)
            ^ (v276 << 76)
            ^ (v277 << 77)
            ^ (v278 << 78)
            ^ (v279 << 79)
            ^ (v280 << 80)
            ^ (v281 << 81)
            ^ (v282 << 82)
            ^ (v283 << 83)
            ^ (v284 << 84)
            ^ (v285 << 85)
            ^ (v286 << 86)
            ^ (v287 << 87)
            ^ (v288 << 88)
            ^ (v289 << 89)
            ^ (v290 << 90)
            ^ (v291 << 91)
            ^ (v292 << 92)
            ^ (v293 << 93)
            ^ (v294 << 94)
            ^ (v295 << 95)
            ^ (v296 << 96)
            ^ (v297 << 97)
            ^ (v298 << 98)
            ^ (v299 << 99);
        assembly {
            mstore(0, r)
            return(0, 0x20)
        }
    }
}

contract Carried18Harness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new Carried18()).call(
            abi.encodePacked(bytes4(0x12345678), new bytes(700))
        );
        require(
            success
                && abi.decode(result, (uint256))
                    == 0x3708d80e13,
            "carried calls"
        );
        return 1;
    }
}

contract Carried40Harness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new Carried40()).call(
            abi.encodePacked(bytes4(0x12345678), new bytes(700))
        );
        require(
            success
                && abi.decode(result, (uint256))
                    == 0x1d18ede9391c0e13,
            "carried calls"
        );
        return 1;
    }
}

contract Carried300Harness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new Carried300()).call(
            abi.encodePacked(bytes4(0x12345678), new bytes(700))
        );
        require(
            success
                && abi.decode(result, (uint256))
                    == 0x42d8b216b90906e9394906e9b761ef1fd00f4fefaeb00fefd067e058,
            "carried calls"
        );
        return 1;
    }
}

// The same across a recursive helper, which takes the dynamic-frame call path.
contract CarriedRecursive {
    function rc(uint256 n) internal returns (uint256) {
        assembly { calldatacopy(0x200, 0, calldatasize()) }
        if (n == 0) return 1;
        return rc(n - 1) + n;
    }

    fallback() external {
        uint256 v0;
        assembly { v0 := add(sload(0), 7) }
        uint256 v1;
        assembly { v1 := add(sload(1), 65544) }
        uint256 v2;
        assembly { v2 := add(sload(2), 131081) }
        uint256 v3;
        assembly { v3 := add(sload(3), 196618) }
        uint256 v4;
        assembly { v4 := add(sload(4), 262155) }
        uint256 v5;
        assembly { v5 := add(sload(5), 327692) }
        uint256 v6;
        assembly { v6 := add(sload(6), 393229) }
        uint256 v7;
        assembly { v7 := add(sload(7), 458766) }
        uint256 v8;
        assembly { v8 := add(sload(8), 524303) }
        uint256 v9;
        assembly { v9 := add(sload(9), 589840) }
        uint256 v10;
        assembly { v10 := add(sload(10), 655377) }
        uint256 v11;
        assembly { v11 := add(sload(11), 720914) }
        uint256 v12;
        assembly { v12 := add(sload(12), 786451) }
        uint256 v13;
        assembly { v13 := add(sload(13), 851988) }
        uint256 v14;
        assembly { v14 := add(sload(14), 917525) }
        uint256 v15;
        assembly { v15 := add(sload(15), 983062) }
        uint256 v16;
        assembly { v16 := add(sload(16), 1048599) }
        uint256 v17;
        assembly { v17 := add(sload(17), 1114136) }
        uint256 v18;
        assembly { v18 := add(sload(18), 1179673) }
        uint256 v19;
        assembly { v19 := add(sload(19), 1245210) }
        assembly { calldatacopy(0x80, 0, calldatasize()) }
        uint256 r = rc(3);
        r ^= v0 << 0;
        r ^= v1 << 1;
        r ^= v2 << 2;
        r ^= v3 << 3;
        r ^= v4 << 4;
        r ^= v5 << 5;
        r ^= v6 << 6;
        r ^= v7 << 7;
        r ^= v8 << 8;
        r ^= v9 << 9;
        r ^= v10 << 10;
        r ^= v11 << 11;
        r ^= v12 << 12;
        r ^= v13 << 13;
        r ^= v14 << 14;
        r ^= v15 << 15;
        r ^= v16 << 16;
        r ^= v17 << 17;
        r ^= v18 << 18;
        r ^= v19 << 19;
        assembly {
            mstore(0, r)
            return(0, 0x20)
        }
    }
}

contract CarriedRecursiveHarness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new CarriedRecursive()).call(
            abi.encodePacked(bytes4(0x12345678), new bytes(700))
        );
        require(success && abi.decode(result, (uint256)) == 0xe7086c0e14, "carried recursive");
        return 1;
    }
}
