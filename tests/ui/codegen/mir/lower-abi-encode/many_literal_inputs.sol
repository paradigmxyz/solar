//@ compile-flags: -Ogas -Zdump=mir -Zmir-pipeline=lower-abi-encode
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Exercise cached proofs for many independent objects in one function.
interface LabelSink { function label(address, string calldata) external view; }

contract ManyLiteralInputs {
    // CHECK-LABEL: fn @run(
    // CHECK-NOT: mcopy
    // CHECK-COUNT-128: staticcall
    // CHECK-NOT: mcopy
    function run(LabelSink sink) external view {
        sink.label(address(0), "label number 0 for the cheatcode call");
        sink.label(address(1), "label number 1 for the cheatcode call");
        sink.label(address(2), "label number 2 for the cheatcode call");
        sink.label(address(3), "label number 3 for the cheatcode call");
        sink.label(address(4), "label number 4 for the cheatcode call");
        sink.label(address(5), "label number 5 for the cheatcode call");
        sink.label(address(6), "label number 6 for the cheatcode call");
        sink.label(address(7), "label number 7 for the cheatcode call");
        sink.label(address(8), "label number 8 for the cheatcode call");
        sink.label(address(9), "label number 9 for the cheatcode call");
        sink.label(address(10), "label number 10 for the cheatcode call");
        sink.label(address(11), "label number 11 for the cheatcode call");
        sink.label(address(12), "label number 12 for the cheatcode call");
        sink.label(address(13), "label number 13 for the cheatcode call");
        sink.label(address(14), "label number 14 for the cheatcode call");
        sink.label(address(15), "label number 15 for the cheatcode call");
        sink.label(address(16), "label number 16 for the cheatcode call");
        sink.label(address(17), "label number 17 for the cheatcode call");
        sink.label(address(18), "label number 18 for the cheatcode call");
        sink.label(address(19), "label number 19 for the cheatcode call");
        sink.label(address(20), "label number 20 for the cheatcode call");
        sink.label(address(21), "label number 21 for the cheatcode call");
        sink.label(address(22), "label number 22 for the cheatcode call");
        sink.label(address(23), "label number 23 for the cheatcode call");
        sink.label(address(24), "label number 24 for the cheatcode call");
        sink.label(address(25), "label number 25 for the cheatcode call");
        sink.label(address(26), "label number 26 for the cheatcode call");
        sink.label(address(27), "label number 27 for the cheatcode call");
        sink.label(address(28), "label number 28 for the cheatcode call");
        sink.label(address(29), "label number 29 for the cheatcode call");
        sink.label(address(30), "label number 30 for the cheatcode call");
        sink.label(address(31), "label number 31 for the cheatcode call");
        sink.label(address(32), "label number 32 for the cheatcode call");
        sink.label(address(33), "label number 33 for the cheatcode call");
        sink.label(address(34), "label number 34 for the cheatcode call");
        sink.label(address(35), "label number 35 for the cheatcode call");
        sink.label(address(36), "label number 36 for the cheatcode call");
        sink.label(address(37), "label number 37 for the cheatcode call");
        sink.label(address(38), "label number 38 for the cheatcode call");
        sink.label(address(39), "label number 39 for the cheatcode call");
        sink.label(address(40), "label number 40 for the cheatcode call");
        sink.label(address(41), "label number 41 for the cheatcode call");
        sink.label(address(42), "label number 42 for the cheatcode call");
        sink.label(address(43), "label number 43 for the cheatcode call");
        sink.label(address(44), "label number 44 for the cheatcode call");
        sink.label(address(45), "label number 45 for the cheatcode call");
        sink.label(address(46), "label number 46 for the cheatcode call");
        sink.label(address(47), "label number 47 for the cheatcode call");
        sink.label(address(48), "label number 48 for the cheatcode call");
        sink.label(address(49), "label number 49 for the cheatcode call");
        sink.label(address(50), "label number 50 for the cheatcode call");
        sink.label(address(51), "label number 51 for the cheatcode call");
        sink.label(address(52), "label number 52 for the cheatcode call");
        sink.label(address(53), "label number 53 for the cheatcode call");
        sink.label(address(54), "label number 54 for the cheatcode call");
        sink.label(address(55), "label number 55 for the cheatcode call");
        sink.label(address(56), "label number 56 for the cheatcode call");
        sink.label(address(57), "label number 57 for the cheatcode call");
        sink.label(address(58), "label number 58 for the cheatcode call");
        sink.label(address(59), "label number 59 for the cheatcode call");
        sink.label(address(60), "label number 60 for the cheatcode call");
        sink.label(address(61), "label number 61 for the cheatcode call");
        sink.label(address(62), "label number 62 for the cheatcode call");
        sink.label(address(63), "label number 63 for the cheatcode call");
        sink.label(address(64), "label number 64 for the cheatcode call");
        sink.label(address(65), "label number 65 for the cheatcode call");
        sink.label(address(66), "label number 66 for the cheatcode call");
        sink.label(address(67), "label number 67 for the cheatcode call");
        sink.label(address(68), "label number 68 for the cheatcode call");
        sink.label(address(69), "label number 69 for the cheatcode call");
        sink.label(address(70), "label number 70 for the cheatcode call");
        sink.label(address(71), "label number 71 for the cheatcode call");
        sink.label(address(72), "label number 72 for the cheatcode call");
        sink.label(address(73), "label number 73 for the cheatcode call");
        sink.label(address(74), "label number 74 for the cheatcode call");
        sink.label(address(75), "label number 75 for the cheatcode call");
        sink.label(address(76), "label number 76 for the cheatcode call");
        sink.label(address(77), "label number 77 for the cheatcode call");
        sink.label(address(78), "label number 78 for the cheatcode call");
        sink.label(address(79), "label number 79 for the cheatcode call");
        sink.label(address(80), "label number 80 for the cheatcode call");
        sink.label(address(81), "label number 81 for the cheatcode call");
        sink.label(address(82), "label number 82 for the cheatcode call");
        sink.label(address(83), "label number 83 for the cheatcode call");
        sink.label(address(84), "label number 84 for the cheatcode call");
        sink.label(address(85), "label number 85 for the cheatcode call");
        sink.label(address(86), "label number 86 for the cheatcode call");
        sink.label(address(87), "label number 87 for the cheatcode call");
        sink.label(address(88), "label number 88 for the cheatcode call");
        sink.label(address(89), "label number 89 for the cheatcode call");
        sink.label(address(90), "label number 90 for the cheatcode call");
        sink.label(address(91), "label number 91 for the cheatcode call");
        sink.label(address(92), "label number 92 for the cheatcode call");
        sink.label(address(93), "label number 93 for the cheatcode call");
        sink.label(address(94), "label number 94 for the cheatcode call");
        sink.label(address(95), "label number 95 for the cheatcode call");
        sink.label(address(96), "label number 96 for the cheatcode call");
        sink.label(address(97), "label number 97 for the cheatcode call");
        sink.label(address(98), "label number 98 for the cheatcode call");
        sink.label(address(99), "label number 99 for the cheatcode call");
        sink.label(address(100), "label number 100 for the cheatcode call");
        sink.label(address(101), "label number 101 for the cheatcode call");
        sink.label(address(102), "label number 102 for the cheatcode call");
        sink.label(address(103), "label number 103 for the cheatcode call");
        sink.label(address(104), "label number 104 for the cheatcode call");
        sink.label(address(105), "label number 105 for the cheatcode call");
        sink.label(address(106), "label number 106 for the cheatcode call");
        sink.label(address(107), "label number 107 for the cheatcode call");
        sink.label(address(108), "label number 108 for the cheatcode call");
        sink.label(address(109), "label number 109 for the cheatcode call");
        sink.label(address(110), "label number 110 for the cheatcode call");
        sink.label(address(111), "label number 111 for the cheatcode call");
        sink.label(address(112), "label number 112 for the cheatcode call");
        sink.label(address(113), "label number 113 for the cheatcode call");
        sink.label(address(114), "label number 114 for the cheatcode call");
        sink.label(address(115), "label number 115 for the cheatcode call");
        sink.label(address(116), "label number 116 for the cheatcode call");
        sink.label(address(117), "label number 117 for the cheatcode call");
        sink.label(address(118), "label number 118 for the cheatcode call");
        sink.label(address(119), "label number 119 for the cheatcode call");
        sink.label(address(120), "label number 120 for the cheatcode call");
        sink.label(address(121), "label number 121 for the cheatcode call");
        sink.label(address(122), "label number 122 for the cheatcode call");
        sink.label(address(123), "label number 123 for the cheatcode call");
        sink.label(address(124), "label number 124 for the cheatcode call");
        sink.label(address(125), "label number 125 for the cheatcode call");
        sink.label(address(126), "label number 126 for the cheatcode call");
        sink.label(address(127), "label number 127 for the cheatcode call");
    }
}
