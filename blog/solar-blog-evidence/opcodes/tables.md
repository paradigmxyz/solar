# Execution gas by opcode kind

Every unit of execution gas in every hot-profile runtime benchmark call, attributed by an opcode step tracer (`tracer.js` through anvil `debug_traceTransaction`). Execution gas is receipt gas minus intrinsic gas plus applied refund; it is what codegen controls. Calls where the EIP-7623 calldata floor sets receipt gas still report their real execution gas. Only calls that succeeded under all three compilers of an objective, and that the runtime bench does not exclude from comparisons, enter the comparisons.


## gas: -Ogas / default runs

### Coverage

Compared calls: 515 across 26 cases. Attribution check (sum of step gas + intrinsic - refund = receipt gas, or receipt is the calldata floor): 0 mismatches; 313 calls billed at the calldata floor.

| case | compared/total calls | solar | solc-legacy | solc-viair |
| --- | ---: | --- | --- | --- |
| solady-lib-string | 48/70 | ok | ok | ok |
| tempo-zones-portal | 0/2 | ok | failed: CompilerError: Stack too deep. Try compiling with `--via-ir` (cli) or the equiva | ok |

### Totals and kinds (sum over compared calls)

| kind | solar gas | share | solc-legacy gas | share | solc-viair gas | share | solar vs legacy | solar vs via-IR |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| stack | 1,130,847 | 21.1% | 1,780,571 | 23.4% | 1,590,615 | 21.3% | -649,724 (-36.5%) | -459,768 (-28.9%) |
| push | 763,476 | 14.2% | 1,085,136 | 14.3% | 1,150,765 | 15.4% | -321,660 (-29.6%) | -387,289 (-33.7%) |
| recipe | 5,412 | 0.1% | 57,927 | 0.8% | 147,309 | 2.0% | -52,515 (-90.7%) | -141,897 (-96.3%) |
| control | 633,905 | 11.8% | 1,254,593 | 16.5% | 1,260,397 | 16.9% | -620,688 (-49.5%) | -626,492 (-49.7%) |
| guard | 166,035 | 3.1% | 371,235 | 4.9% | 355,889 | 4.8% | -205,200 (-55.3%) | -189,854 (-53.3%) |
| arith | 568,307 | 10.6% | 574,537 | 7.6% | 601,208 | 8.0% | -6,230 (-1.1%) | -32,901 (-5.5%) |
| compare | 68,457 | 1.3% | 227,121 | 3.0% | 127,224 | 1.7% | -158,664 (-69.9%) | -58,767 (-46.2%) |
| bitwise | 243,894 | 4.5% | 254,151 | 3.3% | 263,697 | 3.5% | -10,257 (-4.0%) | -19,803 (-7.5%) |
| memory | 524,229 | 9.8% | 541,737 | 7.1% | 542,424 | 7.3% | -17,508 (-3.2%) | -18,195 (-3.4%) |
| fmp | 16,845 | 0.3% | 26,295 | 0.3% | 23,355 | 0.3% | -9,450 (-35.9%) | -6,510 (-27.9%) |
| mem_expansion | 34,778 | 0.6% | 40,494 | 0.5% | 40,596 | 0.5% | -5,716 (-14.1%) | -5,818 (-14.3%) |
| storage | 1,029,600 | 19.2% | 1,208,600 | 15.9% | 1,192,300 | 16.0% | -179,000 (-14.8%) | -162,700 (-13.6%) |
| calldata | 27,977 | 0.5% | 24,418 | 0.3% | 24,653 | 0.3% | +3,559 (+14.6%) | +3,324 (+13.5%) |
| returndata | 233 | 0.0% | 254 | 0.0% | 246 | 0.0% | -21 (-8.3%) | -13 (-5.3%) |
| keccak | 15,294 | 0.3% | 16,512 | 0.2% | 16,512 | 0.2% | -1,218 (-7.4%) | -1,218 (-7.4%) |
| call | 68,200 | 1.3% | 68,200 | 0.9% | 68,200 | 0.9% | +0 (+0.0%) | +0 (+0.0%) |
| precompile | 16,320 | 0.3% | 16,320 | 0.2% | 16,320 | 0.2% | +0 (+0.0%) | +0 (+0.0%) |
| log | 42,400 | 0.8% | 42,400 | 0.6% | 42,400 | 0.6% | +0 (+0.0%) | +0 (+0.0%) |
| env | 9,538 | 0.2% | 9,785 | 0.1% | 10,062 | 0.1% | -247 (-2.5%) | -524 (-5.2%) |
| **total** | **5,365,747** | 100% | **7,600,286** | 100% | **7,474,172** | 100% | -2,234,539 (-29.4%) | -2,108,425 (-28.2%) |
| steps | 1,245,305 |  | 1,842,192 |  | 1,792,419 |  | -596,887 (-32.4%) | -547,114 (-30.5%) |

Overhead kinds (stack, push, recipe, control, guard, fmp): solar 2,716,520 (50.6%); solc-legacy 4,575,757 (60.2%); solc-viair 4,528,330 (60.6%).

### Dynamic gas components

| component | solar | solc-legacy | solc-viair | top opcodes (solar) |
| --- | ---: | ---: | ---: | --- |
| cold_access | 357,600 | 357,600 | 357,600 | SLOAD 220,000, SSTORE 65,100, STATICCALL 52,500, DELEGATECALL 12,500 |
| copy_words | 17,124 | 9,276 | 9,315 | CALLDATACOPY 9,012, MCOPY 7,848, CODECOPY 144, RETURNDATACOPY 120 |
| exp_bytes | 0 | 12,650 | 0 |  |
| keccak_words | 5,514 | 5,862 | 5,862 | KECCAK256 5,514 |
| log_data | 6,400 | 6,400 | 6,400 | LOG3 5,376, LOG2 1,024 |
| mem_expansion | 34,778 | 40,494 | 40,596 | MSTORE 18,278, CALLDATACOPY 10,295, MSTORE8 4,512, STATICCALL 1,041 |
| precompile | 16,320 | 16,320 | 16,320 | STATICCALL 16,320 |
| sstore_reset | 109,200 | 137,200 | 137,200 | SSTORE 109,200 |
| sstore_set | 616,900 | 616,900 | 616,900 | SSTORE 616,900 |
| **static (all opcodes)** | 4,201,911 | 6,397,584 | 6,283,979 | |

### Memory and storage redundancy (dynamic detection)

Counts and the static gas of the redundant instruction only (MSTORE/MLOAD 3, SLOAD 100); the instructions that compute addresses and values are not included. `same_block` means no JUMPDEST, jump, or call between the two accesses. Dead stores: a 32-byte MSTORE fully overwritten before any read; unread at exit: never read before the frame ended.

| metric | solar count | gas | solc-legacy count | gas | solc-viair count | gas |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| dead_store_overwritten | 162 | 486 | 196 | 588 | 183 | 549 |
| dead_store_overwritten_fmp | 15 | 45 | 0 | 0 | 6 | 18 |
| load_after_store_same_block | 12 | 36 | 201 | 603 | 426 | 1,278 |
| load_after_store_same_block_fmp | 5 | 15 | 41 | 123 | 49 | 147 |
| repeated_load_same_block | 23,809 | 71,427 | 23,809 | 71,427 | 23,809 | 71,427 |
| repeated_load_same_block_fmp | 0 | 0 | 69 | 207 | 2 | 6 |
| unread_at_exit | 700 | 2,100 | 934 | 2,802 | 959 | 2,877 |
| unread_at_exit_fmp | 194 | 582 | 68 | 204 | 74 | 222 |

### Kinds by opcode (top contributors)

| kind | solar | solc-legacy | solc-viair |
| --- | --- | --- | --- |
| stack | DUP1 234,174, DUP2 196,308, DUP4 186,309, DUP3 94,944, DUP5 84,549 | DUP5 341,565, DUP2 326,859, DUP3 232,710, POP 189,986, SWAP1 118,512 | DUP2 320,679, DUP3 243,195, SWAP1 240,498, SWAP2 226,542, DUP4 171,489 |
| push | PUSH1 583,011, PUSH2 160,512, PUSH32 7,233, PUSH0 5,208, PUSH4 4,122 | PUSH1 659,724, PUSH2 385,398, PUSH0 20,616, PUSH4 9,084, PUSH32 7,563 | PUSH1 700,428, PUSH2 297,741, PUSH3 90,630, PUSH4 40,560, PUSH0 12,196 |
| recipe | NOT 3,828, SHL 897, SHR 507, ADD 171, LT 9 | SHL 24,375, SUB 19,995, NOT 13,203, AND 264, DIV 90 | SHL 60,840, NOT 50,484, SUB 35,784, AND 201 |
| control | JUMPI 517,580, JUMPDEST 60,861, JUMP 55,464 | JUMP 591,528, JUMPI 551,590, JUMPDEST 111,475 | JUMPI 587,980, JUMP 552,832, JUMPDEST 119,585 |
| guard | JUMPI 94,200, PUSH1 23,283, LT 18,171, OR 7,788, PUSH2 7,002 | JUMPI 203,180, PUSH2 57,891, ISZERO 32,667, LT 27,189, GT 15,210 | JUMPI 193,790, PUSH2 46,170, ISZERO 33,195, LT 24,180, GT 18,588 |
| arith | ADD 323,418, SUB 112,518, MULMOD 85,168, ADDMOD 24,768, MUL 11,370 | ADD 383,391, MULMOD 89,368, MUL 26,930, ADDMOD 24,768, SUB 16,755 | ADD 376,686, SUB 87,651, MULMOD 85,168, ADDMOD 24,768, DIV 14,700 |
| compare | LT 26,529, GT 22,641, ISZERO 11,742, EQ 7,527, SLT 18 | EQ 78,327, ISZERO 76,485, LT 47,286, GT 24,936, SLT 87 | LT 47,343, ISZERO 37,881, GT 23,664, EQ 18,318, SLT 18 |
| bitwise | AND 148,911, SHR 79,923, BYTE 6,972, SHL 4,680, OR 1,632 | AND 161,346, SHR 78,396, SHL 5,205, BYTE 4,746, OR 2,106 | AND 159,414, SHR 78,867, SHL 16,314, BYTE 4,644, OR 2,106 |
| memory | MLOAD 336,678, MSTORE8 142,305, MSTORE 36,648, MCOPY 8,598 | MLOAD 347,082, MSTORE8 142,305, MSTORE 46,962, MCOPY 5,388 | MLOAD 347,265, MSTORE8 142,305, MSTORE 47,466, MCOPY 5,388 |
| fmp | PUSH1 8,211, MSTORE 3,396, MLOAD 2,820, ADD 2,169, PUSH2 249 | PUSH1 12,648, MLOAD 6,228, MSTORE 4,482, ADD 2,775, PUSH2 162 | PUSH1 11,286, MSTORE 4,569, MLOAD 4,422, ADD 2,940, PUSH2 138 |
| storage | SSTORE 798,600, SLOAD 231,000 | SSTORE 908,900, SLOAD 299,700 | SSTORE 892,900, SLOAD 299,400 |
| calldata | CALLDATALOAD 14,730, CALLDATACOPY 10,389, CALLDATASIZE 2,858 | CALLDATALOAD 17,523, CALLDATACOPY 4,803, CALLDATASIZE 2,092 | CALLDATALOAD 17,508, CALLDATACOPY 4,803, CALLDATASIZE 2,342 |
| returndata | RETURNDATACOPY 153, RETURNDATASIZE 80 | RETURNDATACOPY 150, RETURNDATASIZE 104 | RETURNDATACOPY 150, RETURNDATASIZE 96 |
| keccak | KECCAK256 15,294 | KECCAK256 16,512 | KECCAK256 16,512 |
| call | STATICCALL 55,200, DELEGATECALL 13,000 | STATICCALL 55,200, DELEGATECALL 13,000 | STATICCALL 55,200, DELEGATECALL 13,000 |
| log | LOG3 36,876, LOG2 5,524 | LOG3 36,876, LOG2 5,524 | LOG3 36,876, LOG2 5,524 |
| env | EXTCODESIZE 8,100, CALLVALUE 1,034, CODECOPY 180, SELFBALANCE 80, CALLER 76 | EXTCODESIZE 8,100, CALLVALUE 1,034, CODECOPY 435, SELFBALANCE 80, CALLER 68 | EXTCODESIZE 8,100, CALLVALUE 1,034, CODECOPY 684, CALLER 84, SELFBALANCE 80 |

### Opcodes (top 40 by largest gas in any compiler)

| opcode | solar count | gas | static | solc-legacy count | gas | static | solc-viair count | gas | static |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| SSTORE | 74 | 798,600 | 7,400 | 897 | 908,900 | 89,700 | 737 | 892,900 | 73,700 |
| JUMPI | 61,178 | 611,780 | 611,780 | 75,477 | 754,770 | 754,770 | 78,177 | 781,770 | 781,770 |
| PUSH1 | 204,835 | 614,505 | 614,505 | 227,180 | 681,540 | 681,540 | 240,372 | 721,116 | 721,116 |
| JUMP | 6,933 | 55,464 | 55,464 | 73,941 | 591,528 | 591,528 | 69,104 | 552,832 | 552,832 |
| PUSH2 | 55,921 | 167,763 | 167,763 | 147,817 | 443,451 | 443,451 | 114,683 | 344,049 | 344,049 |
| ADD | 108,586 | 325,758 | 325,758 | 128,722 | 386,166 | 386,166 | 126,542 | 379,626 | 379,626 |
| MLOAD | 113,166 | 339,691 | 339,498 | 117,770 | 353,319 | 353,310 | 117,229 | 351,696 | 351,687 |
| DUP5 | 28,183 | 84,549 | 84,549 | 113,855 | 341,565 | 341,565 | 12,663 | 37,989 | 37,989 |
| DUP2 | 65,436 | 196,308 | 196,308 | 108,953 | 326,859 | 326,859 | 106,893 | 320,679 | 320,679 |
| SLOAD | 110 | 231,000 | 11,000 | 797 | 299,700 | 79,700 | 794 | 299,400 | 79,400 |
| DUP3 | 31,648 | 94,944 | 94,944 | 77,570 | 232,710 | 232,710 | 81,065 | 243,195 | 243,195 |
| SWAP1 | 16,139 | 48,417 | 48,417 | 39,504 | 118,512 | 118,512 | 80,166 | 240,498 | 240,498 |
| DUP1 | 78,058 | 234,174 | 234,174 | 33,441 | 100,323 | 100,323 | 34,969 | 104,907 | 104,907 |
| SWAP2 | 18,774 | 56,322 | 56,322 | 25,521 | 76,563 | 76,563 | 75,514 | 226,542 | 226,542 |
| POP | 12,861 | 25,722 | 25,722 | 94,993 | 189,986 | 189,986 | 14,505 | 29,010 | 29,010 |
| DUP4 | 62,103 | 186,309 | 186,309 | 28,285 | 84,855 | 84,855 | 57,163 | 171,489 | 171,489 |
| AND | 49,645 | 148,935 | 148,935 | 54,904 | 164,712 | 164,712 | 53,238 | 159,714 | 159,714 |
| MSTORE8 | 47,435 | 146,817 | 142,305 | 47,435 | 146,815 | 142,305 | 47,435 | 146,815 | 142,305 |
| SUB | 37,506 | 112,518 | 112,518 | 12,250 | 36,750 | 36,750 | 41,145 | 123,435 | 123,435 |
| JUMPDEST | 60,861 | 60,861 | 60,861 | 111,475 | 111,475 | 111,475 | 119,585 | 119,585 | 119,585 |
| ISZERO | 6,010 | 18,030 | 18,030 | 36,384 | 109,152 | 109,152 | 23,692 | 71,076 | 71,076 |
| PUSH3 | 416 | 1,248 | 1,248 | 480 | 1,440 | 1,440 | 32,927 | 98,781 | 98,781 |
| SWAP4 | 26,254 | 78,762 | 78,762 | 31,876 | 95,628 | 95,628 | 9,355 | 28,065 | 28,065 |
| MULMOD | 10,646 | 85,168 | 85,168 | 11,171 | 89,368 | 89,368 | 10,646 | 85,168 | 85,168 |
| EQ | 3,140 | 9,420 | 9,420 | 29,061 | 87,183 | 87,183 | 8,613 | 25,839 | 25,839 |
| SHR | 26,810 | 80,430 | 80,430 | 26,132 | 78,396 | 78,396 | 26,289 | 78,867 | 78,867 |
| MSTORE | 13,348 | 58,322 | 40,044 | 17,148 | 77,053 | 51,444 | 17,345 | 78,007 | 52,035 |
| SHL | 1,859 | 5,577 | 5,577 | 9,860 | 29,580 | 29,580 | 25,718 | 77,154 | 77,154 |
| LT | 14,903 | 44,709 | 44,709 | 24,825 | 74,475 | 74,475 | 23,841 | 71,523 | 71,523 |
| STATICCALL | 27 | 72,561 | 2,700 | 27 | 72,421 | 2,700 | 27 | 72,421 | 2,700 |
| SWAP3 | 14,454 | 43,362 | 43,362 | 18,886 | 56,658 | 56,658 | 17,833 | 53,499 | 53,499 |
| NOT | 1,343 | 4,029 | 4,029 | 4,596 | 13,788 | 13,788 | 17,023 | 51,069 | 51,069 |
| DUP6 | 4,725 | 14,175 | 14,175 | 15,250 | 45,750 | 45,750 | 5,619 | 16,857 | 16,857 |
| GT | 9,275 | 27,825 | 27,825 | 13,382 | 40,146 | 40,146 | 14,084 | 42,252 | 42,252 |
| PUSH4 | 1,470 | 4,410 | 4,410 | 3,154 | 9,462 | 9,462 | 13,628 | 40,884 | 40,884 |
| LOG3 | 21 | 36,876 | 31,500 | 21 | 36,876 | 31,500 | 21 | 36,876 | 31,500 |
| DUP7 | 3,124 | 9,372 | 9,372 | 9,257 | 27,771 | 27,771 | 10,366 | 31,098 | 31,098 |
| MUL | 2,274 | 11,370 | 11,370 | 5,386 | 26,930 | 26,930 | 1,802 | 9,010 | 9,010 |
| ADDMOD | 3,096 | 24,768 | 24,768 | 3,096 | 24,768 | 24,768 | 3,096 | 24,768 | 24,768 |
| CALLDATACOPY | 459 | 20,684 | 1,377 | 257 | 8,896 | 771 | 257 | 8,896 | 771 |

### Per case: execution gas and the kinds that differ most

Solar minus the cheaper solc pipeline (per kind, positive = Solar spends more).

| case | calls | solar | legacy | via-IR | solar vs best | biggest Solar excess kinds | biggest Solar savings kinds |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| aave-l2-encoder | 23 | 99,700 | 154,386 | 134,133 | -34,433 (-25.7%) vs viair | calldata +1,234, mem_expansion +45 | stack -8,449, control -7,000, push -6,969 |
| arithmetic | 3 | 51,330 | 151,978 | 97,251 | -45,921 (-47.2%) vs viair |  | storage -34,800, guard -4,057, stack -3,375 |
| calldata-loops | 5 | 232,527 | 627,126 | 585,593 | -353,066 (-60.3%) vs viair | arith +2,000, calldata +7 | control -124,202, stack -89,550, guard -68,567 |
| compiler-optimizations | 19 | 67,152 | 98,915 | 89,147 | -21,995 (-24.7%) vs viair | calldata +1 | control -6,065, storage -5,900, stack -3,818 |
| counter | 5 | 42,578 | 43,135 | 42,901 | -323 (-0.8%) vs viair |  | guard -131, mem_expansion -45, push -34 |
| counter-loop | 6 | 115,933 | 268,131 | 154,252 | -38,319 (-24.8%) vs viair |  | guard -21,210, stack -16,734, control -135 |
| division-words | 10 | 61,084 | 220,737 | 163,273 | -102,189 (-62.6%) vs viair |  | stack -33,016, control -32,046, guard -19,035 |
| factorial | 6 | 58,864 | 124,929 | 110,926 | -52,062 (-46.9%) vs viair | arith +429 | storage -43,800, guard -4,286, push -1,341 |
| lilweb3-ens | 12 | 202,888 | 210,484 | 207,192 | -4,304 (-2.1%) vs viair | calldata +12 | storage -1,200, stack -888, guard -568 |
| lilweb3-flashloan | 17 | 121,304 | 128,606 | 125,617 | -4,313 (-3.4%) vs viair | calldata +7 | storage -1,000, guard -739, stack -603 |
| lilweb3-fractional | 12 | 62,722 | 66,620 | 64,983 | -2,261 (-3.5%) vs viair |  | stack -815, guard -523, control -232 |
| maple-erc20 | 13 | 162,203 | 167,497 | 165,983 | -3,780 (-2.3%) vs viair | calldata +39 | stack -1,481, control -856, push -280 |
| minimal-proxy | 5 | 39,673 | 42,160 | 41,152 | -1,479 (-3.6%) vs viair |  | guard -553, stack -281, memory -168 |
| nitro-one-step-proof | 14 | 39,452 | 55,010 | 67,826 | -15,558 (-28.3%) vs legacy | calldata +602, bitwise +66 | stack -6,856, control -2,894, push -1,794 |
| openzeppelin-erc20-mock | 8 | 207,183 | 211,529 | 208,697 | -1,514 (-0.7%) vs viair | calldata +8 | stack -292, guard -232, push -227 |
| openzeppelin-governor | 21 | 54,756 | 88,194 | 96,219 | -33,438 (-37.9%) vs legacy | calldata +477, bitwise +207 | stack -11,223, control -7,881, push -6,594 |
| openzeppelin-p256 | 4 | 352,245 | 440,952 | 614,935 | -88,707 (-20.1%) vs legacy | bitwise +786, calldata +456 | push -23,324, stack -18,870, recipe -14,811 |
| openzeppelin-vesting-wallet | 16 | 38,670 | 47,990 | 43,448 | -4,778 (-11.0%) vs viair | calldata +38 | control -984, push -914, stack -768 |
| seeded-words | 4 | 13,066 | 22,844 | 20,996 | -7,930 (-37.8%) vs viair |  | stack -3,392, control -2,175, push -791 |
| solady-algorithms | 85 | 389,241 | 583,157 | 596,045 | -193,916 (-33.3%) vs legacy | recipe +750, bitwise +372, calldata +294 | stack -70,307, control -54,956, push -25,836 |
| solady-encoding | 153 | 2,227,830 | 2,638,217 | 2,663,372 | -410,387 (-15.6%) vs legacy | arith +65,979, calldata +306, mem_expansion +16 | control -191,888, stack -128,002, push -73,995 |
| solady-lib-string | 48 | 618,213 | 938,688 | 961,209 | -320,475 (-34.1%) vs legacy | arith +2,067, calldata +141, mem_expansion +30 | stack -78,615, control -75,138, push -59,832 |
| solady-signature-checker | 12 | 14,556 | 18,186 | 20,595 | -3,630 (-20.0%) vs legacy | bitwise +45, arith +15, calldata +3 | stack -1,077, control -963, guard -699 |
| sum-array | 4 | 64,312 | 185,269 | 149,809 | -85,497 (-57.1%) vs viair |  | storage -75,800, push -3,183, control -3,159 |
| verified-words | 3 | 9,448 | 17,236 | 16,541 | -7,093 (-42.9%) vs viair |  | stack -3,140, control -1,624, bitwise -960 |
| word-recipes | 7 | 18,817 | 48,310 | 32,077 | -13,260 (-41.3%) vs viair |  | stack -5,343, control -4,109, push -1,372 |

### Per case and kind: Solar minus via-IR (gas)

| case | stack | push | recipe | control | guard | arith | compare | bitwise | memory | fmp | mem_expansion | storage | calldata | returndata | keccak | call | precompile | log | env |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| aave-l2-encoder | -8,449 | -6,969 | -3,456 | -7,000 | -4,894 | -2,202 | -249 | -222 | -1,035 | -1,236 | 45 | 0 | 1,234 | 0 | 0 | 0 | 0 | 0 | 0 |
| arithmetic | -3,375 | -1,123 | 0 | -1,259 | -4,057 | -776 | -480 | 0 | 0 | -18 | -27 | -34,800 | -6 | 0 | 0 | 0 | 0 | 0 | 0 |
| calldata-loops | -89,550 | -52,477 | -3,090 | -124,202 | -68,567 | 2,000 | -8,124 | -8,928 | 0 | -75 | -60 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 |
| compiler-optimizations | -3,818 | -2,142 | -36 | -6,065 | -1,700 | -106 | -210 | 0 | -1,488 | -297 | -234 | -5,900 | 1 | 0 | 0 | 0 | 0 | 0 | 0 |
| counter | -28 | -34 | -21 | -5 | -131 | -15 | 0 | 0 | 0 | -30 | -45 | 0 | -14 | 0 | 0 | 0 | 0 | 0 | 0 |
| counter-loop | -16,734 | -30 | -18 | -135 | -21,210 | -18 | 0 | 0 | 0 | -90 | -72 | 0 | -12 | 0 | 0 | 0 | 0 | 0 | 0 |
| division-words | -33,016 | -12,024 | -279 | -32,046 | -19,035 | -3,708 | -1,794 | 0 | 0 | -147 | -120 | 0 | -20 | 0 | 0 | 0 | 0 | 0 | 0 |
| factorial | -1,329 | -1,341 | -465 | -1,150 | -4,286 | 429 | 0 | 0 | 0 | -54 | -54 | -43,800 | -12 | 0 | 0 | 0 | 0 | 0 | 0 |
| lilweb3-ens | -888 | -360 | -84 | -76 | -568 | -336 | 0 | -48 | -180 | -72 | 0 | -1,200 | 12 | 0 | -504 | 0 | 0 | 0 | 0 |
| lilweb3-flashloan | -603 | -464 | -285 | -287 | -739 | -66 | 0 | -45 | -60 | -192 | -159 | -1,000 | 7 | 0 | -420 | 0 | 0 | 0 | 0 |
| lilweb3-fractional | -815 | -181 | -111 | -232 | -523 | -189 | 0 | -15 | -36 | -36 | -99 | 0 | -24 | 0 | 0 | 0 | 0 | 0 | 0 |
| maple-erc20 | -1,481 | -280 | -231 | -856 | -260 | -90 | -117 | -114 | 0 | -273 | -117 | 0 | 39 | 0 | 0 | 0 | 0 | 0 | 0 |
| minimal-proxy | -281 | -118 | -27 | -70 | -553 | -132 | -15 | -33 | -168 | -3 | -44 | 0 | -22 | -13 | 0 | 0 | 0 | 0 | 0 |
| nitro-one-step-proof | -5,838 | -4,938 | -1,446 | -7,330 | -4,722 | -924 | -126 | -216 | -900 | -1,650 | -870 | 0 | 586 | 0 | 0 | 0 | 0 | 0 | 0 |
| openzeppelin-erc20-mock | -292 | -227 | -138 | -130 | -232 | -48 | -24 | -33 | -24 | -126 | -72 | 0 | 8 | 0 | -168 | 0 | 0 | 0 | -8 |
| openzeppelin-governor | -10,815 | -8,964 | -2,520 | -13,635 | -1,377 | -1,353 | -2,394 | 54 | -495 | -252 | -177 | 0 | 465 | 0 | 0 | 0 | 0 | 0 | 0 |
| openzeppelin-p256 | -16,373 | -108,242 | -97,347 | -27,761 | -11,530 | -825 | 648 | -459 | -633 | -558 | -12 | 0 | 456 | 0 | 0 | 0 | 0 | 0 | -54 |
| openzeppelin-vesting-wallet | -768 | -914 | -570 | -984 | -738 | -96 | -90 | -24 | 0 | -240 | -192 | -200 | 38 | 0 | 0 | 0 | 0 | 0 | 0 |
| seeded-words | -3,392 | -791 | -12 | -2,175 | -109 | -570 | -15 | -768 | 0 | -42 | -48 | 0 | -8 | 0 | 0 | 0 | 0 | 0 | 0 |
| solady-algorithms | -50,653 | -42,231 | -14,478 | -60,551 | -3,973 | -16,716 | -7,560 | 348 | -6,939 | -384 | -3,315 | 0 | 278 | 0 | 0 | 0 | 0 | 0 | -630 |
| solady-encoding | -141,791 | -71,223 | -612 | -209,545 | -8,262 | -3,054 | -153 | -459 | -459 | -306 | 16 | 0 | 306 | 0 | 0 | 0 | 0 | 0 | 0 |
| solady-lib-string | -59,652 | -65,790 | -15,264 | -120,225 | -30,375 | -3,531 | -35,442 | -6,957 | -5,742 | -261 | 30 | 0 | 45 | 0 | 0 | 0 | 0 | 0 | 168 |
| solady-signature-checker | -1,320 | -1,161 | -117 | -1,881 | -696 | -75 | -567 | 27 | -36 | -36 | -36 | 0 | -15 | 0 | -126 | 0 | 0 | 0 | 0 |
| sum-array | -24 | -3,183 | -1,065 | -3,159 | -1,121 | -12 | -1,065 | 0 | 0 | -24 | -36 | -75,800 | -8 | 0 | 0 | 0 | 0 | 0 | 0 |
| verified-words | -3,140 | -710 | -201 | -1,624 | -63 | -116 | -210 | -960 | 0 | -30 | -36 | 0 | -3 | 0 | 0 | 0 | 0 | 0 | 0 |
| word-recipes | -5,343 | -1,372 | -24 | -4,109 | -133 | -372 | -780 | -951 | 0 | -78 | -84 | 0 | -14 | 0 | 0 | 0 | 0 | 0 | 0 |

### Solar's costliest blocks across all compared calls

Straight-line blocks (split at JUMPDEST and after jumps and halts) of the benchmarked contracts, by gas over the case's compared calls. Overhead = stack, PUSH, JUMP/JUMPI/JUMPDEST gas inside the block. Full listings are in the hot-blocks file.

| case | block | entered | gas | overhead | share of case | shape |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| solady-encoding | @417 | 22,962 | 2,116,998 | 1,216,986 (57.5%) | 95.0% | `JUMPDEST PUSH1 ADD DUP1 MLOAD PUSH1 AND MLOAD DUP4 PUSH1 ADD MSTORE8 DUP1 MLOAD ...` |
| solady-algorithms | @946 | 4,110 | 205,500 | 143,850 (70.0%) | 52.8% | `JUMPDEST PUSH1 DUP1 DUP4 SUB SWAP3 ADD MSTORE DUP1 MLOAD DUP3 DUP2 GT PUSH2 ...` |
| openzeppelin-p256 | @1145 | 253 | 176,594 | 95,128 (53.9%) | 50.1% | `DUP5 DUP4 DUP1 MULMOD DUP6 DUP4 DUP1 MULMOD DUP7 SWAP1 DUP1 MULMOD PUSH32 DUP8 ...` |
| lilweb3-ens | @273 | 8 | 160,200 | 64 (0.0%) | 79.0% | `PUSH0 NOT PUSH1 SHR NOT AND CALLER OR SWAP1 SSTORE STOP` |
| maple-erc20 | @579 | 13 | 141,447 | 702 (0.5%) | 87.2% | `JUMPDEST DUP2 PUSH0 MSTORE PUSH1 PUSH1 MSTORE PUSH1 PUSH0 KECCAK256 DUP4 PUSH0 MSTORE PUSH1 ...` |
| lilweb3-flashloan | @666 | 4 | 94,288 | 124 (0.1%) | 77.7% | `PUSH1 CALLDATALOAD DUP2 PUSH0 MSTORE DUP1 PUSH32 PUSH1 PUSH0 LOG2 PUSH0 MSTORE PUSH0 PUSH1 ...` |
| openzeppelin-erc20-mock | @280 | 5 | 93,787 | 120 (0.1%) | 45.3% | `DUP3 PUSH0 MSTORE PUSH0 PUSH1 MSTORE PUSH1 PUSH0 KECCAK256 DUP1 SLOAD DUP6 ADD SWAP1 ...` |
| lilweb3-fractional | @72 | 7 | 59,941 | 469 (0.8%) | 95.6% | `PUSH1 CALLDATALOAD PUSH0 MSTORE PUSH1 PUSH1 MSTORE PUSH1 PUSH0 KECCAK256 PUSH1 DUP2 ADD SLOAD ...` |
| solady-lib-string | @9236 | 1,536 | 58,377 | 39,936 (68.4%) | 9.4% | `POP SWAP2 PUSH1 ADD SWAP2 DUP3 ADD DUP1 MLOAD PUSH1 DUP3 MSTORE SWAP3` |
| openzeppelin-p256 | @1520 | 175 | 52,675 | 32,375 (61.5%) | 15.0% | `POP DUP6 DUP2 DUP1 MULMOD SWAP2 DUP3 DUP8 SWAP2 MULMOD DUP7 PUSH1 DUP3 MULMOD ...` |
| factorial | @111 | 6 | 47,124 | 24 (0.1%) | 80.1% | `POP PUSH0 SSTORE` |
| calldata-loops | @673 | 512 | 44,544 | 25,600 (57.5%) | 19.2% | `JUMPDEST DUP2 CALLDATALOAD DUP2 CALLDATALOAD DUP1 DUP3 MUL SWAP2 DUP2 ISZERO SWAP2 DUP4 DIV ...` |
| solady-lib-string | @9226 | 1,536 | 44,544 | 35,328 (79.3%) | 7.2% | `JUMPDEST DUP2 MLOAD PUSH1 DUP2 ISZERO PUSH1 JUMPI` |
| openzeppelin-erc20-mock | @729 | 2 | 44,506 | 96 (0.2%) | 21.5% | `DUP2 PUSH0 MSTORE PUSH1 PUSH1 MSTORE PUSH1 PUSH0 KECCAK256 DUP4 PUSH0 MSTORE PUSH1 MSTORE ...` |
| openzeppelin-p256 | @1434 | 175 | 43,750 | 23,450 (53.6%) | 12.4% | `JUMPDEST PUSH1 DUP3 ADD MLOAD PUSH16 PUSH1 SHL NOT DUP1 DUP3 DUP1 MULMOD DUP2 ...` |
| solady-lib-string | @9347 | 768 | 40,704 | 29,184 (71.7%) | 6.6% | `JUMPDEST SWAP1 PUSH1 DUP2 DUP1 MLOAD ADD DUP2 ADD SWAP2 ADD DUP2 DUP2 SUB ...` |
| solady-lib-string | @9251 | 1,536 | 39,936 | 30,720 (76.9%) | 6.5% | `JUMPDEST DUP1 MLOAD PUSH32 AND PUSH2 JUMPI` |
| solady-lib-string | @48024 | 657 | 38,799 | 24,966 (64.3%) | 6.3% | `JUMPDEST SWAP3 PUSH1 ADD SWAP3 DUP1 MLOAD PUSH1 SHR MLOAD PUSH1 BYTE ADD DUP2 ...` |
| sum-array | @98 | 4 | 37,120 | 20 (0.1%) | 57.7% | `JUMPDEST POP PUSH0 SSTORE` |
| counter-loop | @166 | 1,110 | 35,520 | 28,860 (81.2%) | 30.6% | `JUMPDEST DUP1 DUP3 ADD SWAP2 DUP3 LT PUSH1 JUMPI` |
| counter | @60 | 3 | 32,124 | 15 (0.0%) | 75.4% | `PUSH1 CALLDATALOAD PUSH0 SSTORE STOP` |
| arithmetic | @131 | 3 | 32,106 | 6 (0.0%) | 62.5% | `PUSH0 SSTORE` |
| openzeppelin-vesting-wallet | @838 | 14 | 29,904 | 350 (1.2%) | 77.3% | `SELFBALANCE PUSH1 SLOAD DUP2 ADD SWAP1 DUP2 LT PUSH1 JUMPI` |
| solady-lib-string | @48995 | 675 | 27,675 | 19,575 (70.7%) | 4.5% | `JUMPDEST DUP1 DUP4 ADD MLOAD PUSH1 BYTE PUSH1 DUP2 LT PUSH2 JUMPI` |
| solady-lib-string | @9305 | 1,536 | 27,648 | 23,040 (83.3%) | 4.5% | `JUMPDEST SWAP3 SWAP1 MSTORE JUMP` |


## size: -Osize / optimizer runs 1

### Coverage

Compared calls: 515 across 26 cases. Attribution check (sum of step gas + intrinsic - refund = receipt gas, or receipt is the calldata floor): 0 mismatches; 274 calls billed at the calldata floor.

| case | compared/total calls | solar | solc-legacy | solc-viair |
| --- | ---: | --- | --- | --- |
| solady-lib-string | 48/70 | ok | ok | ok |
| tempo-zones-portal | 0/2 | ok | failed: CompilerError: Stack too deep. Try compiling with `--via-ir` (cli) or the equiva | ok |

### Totals and kinds (sum over compared calls)

| kind | solar gas | share | solc-legacy gas | share | solc-viair gas | share | solar vs legacy | solar vs via-IR |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| stack | 1,220,840 | 20.1% | 1,792,472 | 23.3% | 1,602,222 | 21.3% | -571,632 (-31.9%) | -381,382 (-23.8%) |
| push | 937,427 | 15.4% | 1,120,074 | 14.5% | 1,163,880 | 15.5% | -182,647 (-16.3%) | -226,453 (-19.5%) |
| recipe | 10,638 | 0.2% | 66,540 | 0.9% | 154,518 | 2.1% | -55,902 (-84.0%) | -143,880 (-93.1%) |
| control | 1,039,510 | 17.1% | 1,295,397 | 16.8% | 1,260,505 | 16.8% | -255,887 (-19.8%) | -220,995 (-17.5%) |
| guard | 157,639 | 2.6% | 367,929 | 4.8% | 352,469 | 4.7% | -210,290 (-57.2%) | -194,830 (-55.3%) |
| arith | 563,651 | 9.3% | 574,639 | 7.5% | 601,289 | 8.0% | -10,988 (-1.9%) | -37,638 (-6.3%) |
| compare | 104,628 | 1.7% | 238,137 | 3.1% | 127,224 | 1.7% | -133,509 (-56.1%) | -22,596 (-17.8%) |
| bitwise | 253,359 | 4.2% | 254,640 | 3.3% | 265,746 | 3.5% | -1,281 (-0.5%) | -12,387 (-4.7%) |
| memory | 524,256 | 8.6% | 541,773 | 7.0% | 542,559 | 7.2% | -17,517 (-3.2%) | -18,303 (-3.4%) |
| fmp | 17,094 | 0.3% | 26,547 | 0.3% | 23,061 | 0.3% | -9,453 (-35.6%) | -5,967 (-25.9%) |
| mem_expansion | 34,802 | 0.6% | 40,494 | 0.5% | 40,596 | 0.5% | -5,692 (-14.1%) | -5,794 (-14.3%) |
| storage | 1,029,800 | 16.9% | 1,208,600 | 15.7% | 1,192,300 | 15.9% | -178,800 (-14.8%) | -162,500 (-13.6%) |
| calldata | 33,077 | 0.5% | 24,418 | 0.3% | 24,653 | 0.3% | +8,659 (+35.5%) | +8,424 (+34.2%) |
| returndata | 233 | 0.0% | 254 | 0.0% | 246 | 0.0% | -21 (-8.3%) | -13 (-5.3%) |
| keccak | 15,294 | 0.3% | 16,512 | 0.2% | 16,512 | 0.2% | -1,218 (-7.4%) | -1,218 (-7.4%) |
| call | 68,200 | 1.1% | 68,200 | 0.9% | 68,200 | 0.9% | +0 (+0.0%) | +0 (+0.0%) |
| precompile | 16,320 | 0.3% | 16,320 | 0.2% | 16,320 | 0.2% | +0 (+0.0%) | +0 (+0.0%) |
| log | 42,400 | 0.7% | 42,400 | 0.6% | 42,400 | 0.6% | +0 (+0.0%) | +0 (+0.0%) |
| env | 9,556 | 0.2% | 9,785 | 0.1% | 10,152 | 0.1% | -229 (-2.3%) | -596 (-5.9%) |
| **total** | **6,078,724** | 100% | **7,705,131** | 100% | **7,504,852** | 100% | -1,626,407 (-21.1%) | -1,426,128 (-19.0%) |
| steps | 1,437,548 |  | 1,868,076 |  | 1,802,510 |  | -430,528 (-23.0%) | -364,962 (-20.2%) |

Overhead kinds (stack, push, recipe, control, guard, fmp): solar 3,383,148 (55.7%); solc-legacy 4,668,959 (60.6%); solc-viair 4,556,655 (60.7%).

### Dynamic gas components

| component | solar | solc-legacy | solc-viair | top opcodes (solar) |
| --- | ---: | ---: | ---: | --- |
| cold_access | 357,600 | 357,600 | 357,600 | SLOAD 220,000, SSTORE 65,100, STATICCALL 52,500, DELEGATECALL 12,500 |
| copy_words | 17,142 | 9,276 | 9,360 | CALLDATACOPY 9,012, MCOPY 7,848, CODECOPY 162, RETURNDATACOPY 120 |
| exp_bytes | 0 | 12,650 | 0 |  |
| keccak_words | 5,514 | 5,862 | 5,862 | KECCAK256 5,514 |
| log_data | 6,400 | 6,400 | 6,400 | LOG3 5,376, LOG2 1,024 |
| mem_expansion | 34,802 | 40,494 | 40,596 | MSTORE 18,467, CALLDATACOPY 10,277, MSTORE8 4,512, STATICCALL 1,041 |
| precompile | 16,320 | 16,320 | 16,320 | STATICCALL 16,320 |
| sstore_reset | 109,200 | 137,200 | 137,200 | SSTORE 109,200 |
| sstore_set | 616,900 | 616,900 | 616,900 | SSTORE 616,900 |
| **static (all opcodes)** | 4,914,846 | 6,502,429 | 6,314,614 | |

### Memory and storage redundancy (dynamic detection)

Counts and the static gas of the redundant instruction only (MSTORE/MLOAD 3, SLOAD 100); the instructions that compute addresses and values are not included. `same_block` means no JUMPDEST, jump, or call between the two accesses. Dead stores: a 32-byte MSTORE fully overwritten before any read; unread at exit: never read before the frame ended.

| metric | solar count | gas | solc-legacy count | gas | solc-viair count | gas |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| dead_store_overwritten | 162 | 486 | 208 | 624 | 192 | 576 |
| dead_store_overwritten_fmp | 15 | 45 | 0 | 0 | 29 | 87 |
| load_after_store_same_block | 12 | 36 | 201 | 603 | 431 | 1,293 |
| load_after_store_same_block_fmp | 8 | 24 | 29 | 87 | 26 | 78 |
| repeated_load_same_block | 23,808 | 71,424 | 23,809 | 71,427 | 23,809 | 71,427 |
| repeated_load_same_block_fmp | 0 | 0 | 46 | 138 | 2 | 6 |
| unread_at_exit | 703 | 2,109 | 934 | 2,802 | 961 | 2,883 |
| unread_at_exit_fmp | 226 | 678 | 68 | 204 | 74 | 222 |

### Kinds by opcode (top contributors)

| kind | solar | solc-legacy | solc-viair |
| --- | --- | --- | --- |
| stack | DUP1 235,923, DUP4 204,609, DUP2 196,743, DUP5 111,486, SWAP4 97,059 | DUP5 341,556, DUP2 327,237, DUP3 232,632, POP 190,142, SWAP1 118,935 | DUP2 320,376, DUP3 248,016, SWAP1 247,305, SWAP2 232,851, DUP4 171,120 |
| push | PUSH1 688,632, PUSH2 214,212, PUSH4 16,161, PUSH0 6,164, PUSH32 5,595 | PUSH1 673,902, PUSH2 397,902, PUSH0 20,622, PUSH4 19,941, PUSH32 5,214 | PUSH1 716,367, PUSH2 297,882, PUSH3 90,657, PUSH4 40,587, PUSH0 11,544 |
| recipe | NOT 6,396, SHL 2,433, SHR 1,578, ADD 231 | SHL 29,829, SUB 23,091, NOT 13,203, AND 327, DIV 90 | SHL 64,644, NOT 50,490, SUB 39,183, AND 201 |
| control | JUMPI 557,710, JUMP 379,136, JUMPDEST 102,664 | JUMP 595,784, JUMPI 587,860, JUMPDEST 111,753 | JUMPI 587,980, JUMP 552,928, JUMPDEST 119,597 |
| guard | JUMPI 87,310, PUSH1 23,517, LT 18,216, OR 9,804, ISZERO 6,672 | JUMPI 202,850, PUSH2 57,792, ISZERO 32,655, LT 27,177, GT 15,210 | JUMPI 193,790, PUSH2 46,170, ISZERO 33,195, LT 24,180, GT 18,588 |
| arith | ADD 330,258, SUB 100,362, MULMOD 85,168, ADDMOD 24,768, MUL 10,950 | ADD 383,463, MULMOD 89,368, MUL 26,930, ADDMOD 24,768, SUB 16,755 | ADD 376,767, SUB 87,651, MULMOD 85,168, ADDMOD 24,768, DIV 14,700 |
| compare | LT 36,570, GT 23,874, ISZERO 23,730, EQ 20,436, SLT 18 | EQ 90,588, ISZERO 76,605, LT 47,352, GT 23,505, SLT 87 | LT 47,343, ISZERO 35,856, GT 23,664, EQ 20,343, SLT 18 |
| bitwise | AND 148,692, SHR 79,998, SHL 12,486, BYTE 6,972, NOT 1,881 | AND 161,403, SHR 78,540, SHL 5,313, BYTE 4,746, OR 2,214 | AND 159,414, SHR 78,867, SHL 18,351, BYTE 4,644, OR 2,106 |
| memory | MLOAD 336,678, MSTORE8 142,305, MSTORE 36,675, MCOPY 8,598 | MLOAD 347,082, MSTORE8 142,305, MSTORE 46,998, MCOPY 5,388 | MLOAD 347,355, MSTORE8 142,305, MSTORE 47,511, MCOPY 5,388 |
| fmp | PUSH1 8,148, MSTORE 3,513, MLOAD 2,841, ADD 2,274, PUSH2 318 | PUSH1 12,846, MLOAD 6,246, MSTORE 4,500, ADD 2,793, PUSH2 162 | PUSH1 11,199, MSTORE 4,569, MLOAD 4,353, ADD 2,871, PUSH2 69 |
| storage | SSTORE 798,600, SLOAD 231,200 | SSTORE 908,900, SLOAD 299,700 | SSTORE 892,900, SLOAD 299,400 |
| calldata | CALLDATALOAD 19,830, CALLDATACOPY 10,389, CALLDATASIZE 2,858 | CALLDATALOAD 17,523, CALLDATACOPY 4,803, CALLDATASIZE 2,092 | CALLDATALOAD 17,508, CALLDATACOPY 4,803, CALLDATASIZE 2,342 |
| returndata | RETURNDATACOPY 153, RETURNDATASIZE 80 | RETURNDATACOPY 150, RETURNDATASIZE 104 | RETURNDATACOPY 150, RETURNDATASIZE 96 |
| keccak | KECCAK256 15,294 | KECCAK256 16,512 | KECCAK256 16,512 |
| call | STATICCALL 55,200, DELEGATECALL 13,000 | STATICCALL 55,200, DELEGATECALL 13,000 | STATICCALL 55,200, DELEGATECALL 13,000 |
| log | LOG3 36,876, LOG2 5,524 | LOG3 36,876, LOG2 5,524 | LOG3 36,876, LOG2 5,524 |
| env | EXTCODESIZE 8,100, CALLVALUE 1,034, CODECOPY 198, SELFBALANCE 80, CALLER 76 | EXTCODESIZE 8,100, CALLVALUE 1,034, CODECOPY 435, SELFBALANCE 80, CALLER 68 | EXTCODESIZE 8,100, CALLVALUE 1,034, CODECOPY 774, CALLER 84, SELFBALANCE 80 |

### Opcodes (top 40 by largest gas in any compiler)

| opcode | solar count | gas | static | solc-legacy count | gas | static | solc-viair count | gas | static |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| SSTORE | 74 | 798,600 | 7,400 | 897 | 908,900 | 89,700 | 737 | 892,900 | 73,700 |
| JUMPI | 64,502 | 645,020 | 645,020 | 79,071 | 790,710 | 790,710 | 78,177 | 781,770 | 781,770 |
| PUSH1 | 240,099 | 720,297 | 720,297 | 231,972 | 695,916 | 695,916 | 245,656 | 736,968 | 736,968 |
| JUMP | 47,392 | 379,136 | 379,136 | 74,473 | 595,784 | 595,784 | 69,116 | 552,928 | 552,928 |
| PUSH2 | 73,077 | 219,231 | 219,231 | 151,952 | 455,856 | 455,856 | 114,707 | 344,121 | 344,121 |
| ADD | 110,921 | 332,763 | 332,763 | 128,752 | 386,256 | 386,256 | 126,546 | 379,638 | 379,638 |
| MLOAD | 113,173 | 339,712 | 339,519 | 117,776 | 353,337 | 353,328 | 117,236 | 351,717 | 351,708 |
| DUP5 | 37,162 | 111,486 | 111,486 | 113,852 | 341,556 | 341,556 | 13,604 | 40,812 | 40,812 |
| DUP2 | 65,581 | 196,743 | 196,743 | 109,079 | 327,237 | 327,237 | 106,792 | 320,376 | 320,376 |
| SLOAD | 112 | 231,200 | 11,200 | 797 | 299,700 | 79,700 | 794 | 299,400 | 79,400 |
| DUP3 | 31,077 | 93,231 | 93,231 | 77,544 | 232,632 | 232,632 | 82,672 | 248,016 | 248,016 |
| SWAP1 | 27,332 | 81,996 | 81,996 | 39,645 | 118,935 | 118,935 | 82,435 | 247,305 | 247,305 |
| DUP1 | 78,641 | 235,923 | 235,923 | 37,097 | 111,291 | 111,291 | 33,070 | 99,210 | 99,210 |
| SWAP2 | 19,716 | 59,148 | 59,148 | 25,516 | 76,548 | 76,548 | 77,617 | 232,851 | 232,851 |
| DUP4 | 68,203 | 204,609 | 204,609 | 28,237 | 84,711 | 84,711 | 57,040 | 171,120 | 171,120 |
| POP | 18,592 | 37,184 | 37,184 | 95,071 | 190,142 | 190,142 | 14,505 | 29,010 | 29,010 |
| AND | 49,572 | 148,716 | 148,716 | 54,931 | 164,793 | 164,793 | 53,238 | 159,714 | 159,714 |
| MSTORE8 | 47,435 | 146,817 | 142,305 | 47,435 | 146,815 | 142,305 | 47,435 | 146,815 | 142,305 |
| SUB | 33,454 | 100,362 | 100,362 | 13,282 | 39,846 | 39,846 | 42,278 | 126,834 | 126,834 |
| JUMPDEST | 102,664 | 102,664 | 102,664 | 111,753 | 111,753 | 111,753 | 119,597 | 119,597 | 119,597 |
| ISZERO | 10,134 | 30,402 | 30,402 | 36,420 | 109,260 | 109,260 | 23,017 | 69,051 | 69,051 |
| EQ | 7,555 | 22,665 | 22,665 | 33,132 | 99,396 | 99,396 | 9,288 | 27,864 | 27,864 |
| PUSH3 | 416 | 1,248 | 1,248 | 482 | 1,446 | 1,446 | 32,936 | 98,808 | 98,808 |
| SWAP4 | 32,353 | 97,059 | 97,059 | 31,922 | 95,766 | 95,766 | 9,180 | 27,540 | 27,540 |
| MULMOD | 10,646 | 85,168 | 85,168 | 11,171 | 89,368 | 89,368 | 10,646 | 85,168 | 85,168 |
| SHL | 4,973 | 14,919 | 14,919 | 11,714 | 35,142 | 35,142 | 27,665 | 82,995 | 82,995 |
| SHR | 27,192 | 81,576 | 81,576 | 26,180 | 78,540 | 78,540 | 26,289 | 78,867 | 78,867 |
| MSTORE | 13,396 | 58,655 | 40,188 | 17,166 | 77,107 | 51,498 | 17,360 | 78,052 | 52,080 |
| LT | 18,262 | 54,786 | 54,786 | 24,843 | 74,529 | 74,529 | 23,841 | 71,523 | 71,523 |
| STATICCALL | 27 | 72,561 | 2,700 | 27 | 72,421 | 2,700 | 27 | 72,421 | 2,700 |
| SWAP3 | 15,263 | 45,789 | 45,789 | 18,898 | 56,694 | 56,694 | 18,773 | 56,319 | 56,319 |
| NOT | 2,759 | 8,277 | 8,277 | 4,614 | 13,842 | 13,842 | 17,029 | 51,087 | 51,087 |
| DUP6 | 2,046 | 6,138 | 6,138 | 15,254 | 45,762 | 45,762 | 6,284 | 18,852 | 18,852 |
| GT | 9,479 | 28,437 | 28,437 | 12,905 | 38,715 | 38,715 | 14,084 | 42,252 | 42,252 |
| PUSH4 | 5,467 | 16,401 | 16,401 | 6,757 | 20,271 | 20,271 | 13,637 | 40,911 | 40,911 |
| LOG3 | 21 | 36,876 | 31,500 | 21 | 36,876 | 31,500 | 21 | 36,876 | 31,500 |
| DUP7 | 5,548 | 16,644 | 16,644 | 9,258 | 27,774 | 27,774 | 10,714 | 32,142 | 32,142 |
| MUL | 2,190 | 10,950 | 10,950 | 5,386 | 26,930 | 26,930 | 1,802 | 9,010 | 9,010 |
| ADDMOD | 3,096 | 24,768 | 24,768 | 3,096 | 24,768 | 24,768 | 3,096 | 24,768 | 24,768 |
| CALLDATACOPY | 459 | 20,666 | 1,377 | 257 | 8,896 | 771 | 257 | 8,896 | 771 |

### Per case: execution gas and the kinds that differ most

Solar minus the cheaper solc pipeline (per kind, positive = Solar spends more).

| case | calls | solar | legacy | via-IR | solar vs best | biggest Solar excess kinds | biggest Solar savings kinds |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| aave-l2-encoder | 23 | 103,499 | 160,579 | 136,692 | -33,193 (-24.3%) vs viair | calldata +1,234, mem_expansion +45 | stack -9,031, push -7,358, control -5,198 |
| arithmetic | 3 | 51,457 | 151,978 | 97,251 | -45,794 (-47.1%) vs viair |  | storage -34,800, guard -5,657, stack -3,375 |
| calldata-loops | 5 | 287,405 | 627,270 | 585,737 | -298,332 (-50.9%) vs viair | arith +3,056, calldata +1,540 | control -107,862, stack -77,384, guard -73,651 |
| compiler-optimizations | 19 | 70,113 | 99,808 | 89,207 | -19,094 (-21.4%) vs viair | calldata +361 | storage -5,900, control -4,452, stack -4,079 |
| counter | 5 | 42,578 | 43,135 | 42,901 | -323 (-0.8%) vs viair |  | guard -131, mem_expansion -45, push -34 |
| counter-loop | 6 | 128,232 | 268,131 | 154,252 | -26,020 (-16.9%) vs viair | control +8,813, push +3,321 | guard -21,210, stack -16,734, fmp -90 |
| division-words | 10 | 75,322 | 221,062 | 163,273 | -87,951 (-53.9%) vs viair | calldata +1,303 | stack -30,519, control -26,340, guard -19,438 |
| factorial | 6 | 60,866 | 124,929 | 110,926 | -50,060 (-45.1%) vs viair | calldata +435, arith +429 | storage -43,800, guard -4,286, stack -1,794 |
| lilweb3-ens | 12 | 203,328 | 210,916 | 207,624 | -4,296 (-2.1%) vs viair | control +76, calldata +12 | storage -1,200, stack -744, push -576 |
| lilweb3-flashloan | 17 | 121,370 | 128,676 | 125,617 | -4,247 (-3.4%) vs viair | calldata +7 | storage -1,000, guard -739, stack -591 |
| lilweb3-fractional | 12 | 62,857 | 66,805 | 65,178 | -2,321 (-3.6%) vs viair |  | stack -857, guard -493, push -256 |
| maple-erc20 | 13 | 163,300 | 168,495 | 165,983 | -2,683 (-1.6%) vs viair | calldata +39, push +17, compare +12 | stack -1,352, control -314, fmp -273 |
| minimal-proxy | 5 | 39,865 | 42,232 | 41,356 | -1,491 (-3.6%) vs viair | fmp +87 | guard -502, stack -245, push -241 |
| nitro-one-step-proof | 14 | 41,460 | 55,694 | 68,240 | -14,234 (-25.6%) vs legacy | calldata +602, bitwise +120 | stack -6,400, control -2,158, fmp -1,752 |
| openzeppelin-erc20-mock | 8 | 207,725 | 212,011 | 208,871 | -1,146 (-0.5%) vs viair | control +133, compare +42, calldata +8 | stack -244, guard -232, keccak -168 |
| openzeppelin-governor | 21 | 72,261 | 100,539 | 96,516 | -24,255 (-25.1%) vs viair | calldata +465, bitwise +45 | stack -7,908, control -5,550, push -4,641 |
| openzeppelin-p256 | 4 | 418,969 | 441,684 | 621,318 | -22,715 (-5.1%) vs legacy | control +23,040, bitwise +2,445, calldata +456 | recipe -11,811, stack -9,850, guard -8,137 |
| openzeppelin-vesting-wallet | 16 | 39,224 | 49,636 | 43,340 | -4,116 (-9.5%) vs viair | calldata +38 | stack -930, guard -738, control -730 |
| seeded-words | 4 | 15,958 | 22,844 | 20,996 | -5,038 (-24.0%) vs viair |  | stack -3,387, bitwise -768, arith -558 |
| solady-algorithms | 85 | 441,075 | 585,605 | 598,493 | -144,530 (-24.7%) vs legacy | bitwise +372, calldata +294 | stack -49,668, control -39,206, arith -19,833 |
| solady-encoding | 153 | 2,505,224 | 2,643,725 | 2,668,880 | -138,501 (-5.2%) vs legacy | arith +65,979, control +14,872, calldata +306 | stack -127,462, compare -69,951, guard -9,639 |
| solady-lib-string | 48 | 803,670 | 1,007,406 | 972,873 | -169,203 (-17.4%) vs viair | env +132, calldata +45, mem_expansion +30 | push -32,877, control -31,677, guard -30,051 |
| solady-signature-checker | 12 | 18,066 | 21,156 | 20,901 | -2,835 (-13.6%) vs viair |  | stack -798, guard -633, push -432 |
| sum-array | 4 | 66,765 | 185,269 | 149,809 | -83,044 (-55.4%) vs viair | calldata +1,045 | storage -75,800, control -2,812, push -2,130 |
| verified-words | 3 | 11,628 | 17,236 | 16,541 | -4,913 (-29.7%) vs viair |  | stack -3,140, bitwise -960, compare -210 |
| word-recipes | 7 | 26,507 | 48,310 | 32,077 | -5,570 (-17.4%) vs viair | push +745, calldata +370, arith +183 | stack -4,788, bitwise -957, compare -780 |

### Per case and kind: Solar minus via-IR (gas)

| case | stack | push | recipe | control | guard | arith | compare | bitwise | memory | fmp | mem_expansion | storage | calldata | returndata | keccak | call | precompile | log | env |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| aave-l2-encoder | -9,031 | -7,358 | -4,008 | -5,198 | -4,587 | -2,271 | -78 | -222 | -1,035 | -684 | 45 | 0 | 1,234 | 0 | 0 | 0 | 0 | 0 | 0 |
| arithmetic | -3,375 | -652 | 0 | -3 | -5,657 | -776 | -480 | 0 | 0 | -18 | -27 | -34,800 | -6 | 0 | 0 | 0 | 0 | 0 | 0 |
| calldata-loops | -77,384 | -38,946 | -3,162 | -107,862 | -73,651 | 3,056 | -252 | -1,536 | 0 | -75 | -60 | 0 | 1,540 | 0 | 0 | 0 | 0 | 0 | 0 |
| compiler-optimizations | -4,079 | -1,164 | -60 | -4,452 | -1,726 | -106 | -81 | 0 | -1,488 | -189 | -210 | -5,900 | 361 | 0 | 0 | 0 | 0 | 0 | 0 |
| counter | -28 | -34 | -21 | -5 | -131 | -15 | 0 | 0 | 0 | -30 | -45 | 0 | -14 | 0 | 0 | 0 | 0 | 0 | 0 |
| counter-loop | -16,734 | 3,321 | -18 | 8,813 | -21,210 | -18 | 0 | 0 | 0 | -90 | -72 | 0 | -12 | 0 | 0 | 0 | 0 | 0 | 0 |
| division-words | -30,519 | -8,097 | -279 | -26,340 | -19,438 | -2,556 | -1,758 | 0 | 0 | -147 | -120 | 0 | 1,303 | 0 | 0 | 0 | 0 | 0 | 0 |
| factorial | -1,794 | -465 | -465 | -6 | -4,286 | 429 | 0 | 0 | 0 | -54 | -54 | -43,800 | 435 | 0 | 0 | 0 | 0 | 0 | 0 |
| lilweb3-ens | -744 | -576 | -228 | 76 | -496 | -336 | 0 | -48 | -180 | -72 | 0 | -1,200 | 12 | 0 | -504 | 0 | 0 | 0 | 0 |
| lilweb3-flashloan | -591 | -452 | -285 | -245 | -739 | -66 | 0 | -45 | -60 | -192 | -159 | -1,000 | 7 | 0 | -420 | 0 | 0 | 0 | 0 |
| lilweb3-fractional | -857 | -256 | -129 | -187 | -493 | -189 | 0 | -15 | -36 | -36 | -99 | 0 | -24 | 0 | 0 | 0 | 0 | 0 | 0 |
| maple-erc20 | -1,352 | 17 | -231 | -314 | -260 | -90 | 12 | -114 | 0 | -273 | -117 | 0 | 39 | 0 | 0 | 0 | 0 | 0 | 0 |
| minimal-proxy | -245 | -241 | -114 | -70 | -502 | -126 | -15 | -18 | -168 | 87 | -44 | 0 | -22 | -13 | 0 | 0 | 0 | 0 | 0 |
| nitro-one-step-proof | -5,496 | -4,680 | -1,410 | -6,334 | -4,778 | -924 | -54 | -162 | -900 | -1,758 | -870 | 0 | 586 | 0 | 0 | 0 | 0 | 0 | 0 |
| openzeppelin-erc20-mock | -244 | -146 | -138 | 133 | -232 | -48 | 42 | -33 | -78 | -126 | -72 | 0 | 8 | 0 | -168 | 0 | 0 | 0 | -44 |
| openzeppelin-governor | -7,908 | -4,641 | -2,034 | -5,550 | -1,620 | -1,611 | -477 | 45 | -495 | -252 | -177 | 0 | 465 | 0 | 0 | 0 | 0 | 0 | 0 |
| openzeppelin-p256 | -13,194 | -92,875 | -94,929 | 9,708 | -11,254 | -747 | 657 | 1,176 | -633 | -648 | -12 | 0 | 456 | 0 | 0 | 0 | 0 | 0 | -54 |
| openzeppelin-vesting-wallet | -930 | -634 | -432 | -730 | -738 | -138 | -96 | -24 | 0 | -240 | -192 | 0 | 38 | 0 | 0 | 0 | 0 | 0 | 0 |
| seeded-words | -3,387 | -20 | -12 | -46 | -122 | -558 | -27 | -768 | 0 | -42 | -48 | 0 | -8 | 0 | 0 | 0 | 0 | 0 | 0 |
| solady-algorithms | -30,014 | -28,007 | -15,294 | -44,801 | -3,565 | -17,535 | -7,560 | 348 | -6,939 | -384 | -3,315 | 0 | 278 | 0 | 0 | 0 | 0 | 0 | -630 |
| solady-encoding | -141,251 | -5,719 | -2,448 | -2,785 | -7,344 | -3,054 | -153 | -459 | -459 | -306 | 16 | 0 | 306 | 0 | 0 | 0 | 0 | 0 | 0 |
| solady-lib-string | -22,410 | -32,877 | -16,650 | -31,677 | -30,051 | -9,843 | -11,250 | -8,586 | -5,796 | -270 | 30 | 0 | 45 | 0 | 0 | 0 | 0 | 0 | 132 |
| solady-signature-checker | -798 | -432 | -243 | -264 | -633 | -171 | -36 | -9 | -36 | -36 | -36 | 0 | -15 | 0 | -126 | 0 | 0 | 0 | 0 |
| sum-array | -1,089 | -2,130 | -1,065 | -2,812 | -1,121 | -12 | 0 | 0 | 0 | -24 | -36 | -75,800 | 1,045 | 0 | 0 | 0 | 0 | 0 | 0 |
| verified-words | -3,140 | -134 | -201 | -20 | -63 | -116 | -210 | -960 | 0 | -30 | -36 | 0 | -3 | 0 | 0 | 0 | 0 | 0 | 0 |
| word-recipes | -4,788 | 745 | -24 | -24 | -133 | 183 | -780 | -957 | 0 | -78 | -84 | 0 | 370 | 0 | 0 | 0 | 0 | 0 | 0 |

### Solar's costliest blocks across all compared calls

Straight-line blocks (split at JUMPDEST and after jumps and halts) of the benchmarked contracts, by gas over the case's compared calls. Overhead = stack, PUSH, JUMP/JUMPI/JUMPDEST gas inside the block. Full listings are in the hot-blocks file.

| case | block | entered | gas | overhead | share of case | shape |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| solady-encoding | @416 | 22,962 | 1,864,416 | 1,033,290 (55.4%) | 74.4% | `JUMPDEST PUSH1 ADD DUP1 MLOAD PUSH1 AND MLOAD DUP4 PUSH1 ADD MSTORE8 DUP1 MLOAD ...` |
| solady-encoding | @202 | 23,064 | 530,472 | 461,280 (87.0%) | 21.2% | `JUMPDEST DUP5 DUP2 SUB PUSH2 JUMPI` |
| solady-algorithms | @700 | 4,110 | 205,500 | 143,850 (70.0%) | 46.6% | `JUMPDEST PUSH1 DUP5 ADD MSTORE PUSH1 DUP4 SUB DUP1 MLOAD DUP3 DUP2 GT PUSH2 ...` |
| openzeppelin-p256 | @1508 | 510 | 161,160 | 89,760 (55.7%) | 38.5% | `JUMPDEST NOT DUP4 SWAP2 MULMOD DUP3 PUSH1 DUP2 DUP8 DUP1 MULMOD MULMOD DUP4 SWAP2 ...` |
| lilweb3-ens | @337 | 8 | 160,200 | 64 (0.0%) | 78.8% | `PUSH0 NOT PUSH1 SHR NOT AND CALLER OR SWAP1 SSTORE STOP` |
| maple-erc20 | @504 | 13 | 141,447 | 702 (0.5%) | 86.6% | `JUMPDEST DUP2 PUSH0 MSTORE PUSH1 PUSH1 MSTORE PUSH1 PUSH0 KECCAK256 DUP4 PUSH0 MSTORE PUSH1 ...` |
| lilweb3-flashloan | @715 | 4 | 94,288 | 124 (0.1%) | 77.7% | `PUSH1 CALLDATALOAD DUP2 PUSH0 MSTORE DUP1 PUSH32 PUSH1 PUSH0 LOG2 PUSH0 MSTORE PUSH0 PUSH1 ...` |
| openzeppelin-erc20-mock | @244 | 5 | 93,787 | 120 (0.1%) | 45.1% | `DUP3 PUSH0 MSTORE PUSH0 PUSH1 MSTORE PUSH1 PUSH0 KECCAK256 DUP1 SLOAD DUP6 ADD SWAP1 ...` |
| lilweb3-fractional | @72 | 7 | 59,976 | 462 (0.8%) | 95.4% | `PUSH1 CALLDATALOAD PUSH0 MSTORE PUSH1 PUSH1 MSTORE PUSH1 PUSH0 KECCAK256 PUSH1 DUP2 ADD SLOAD ...` |
| solady-lib-string | @5695 | 1,536 | 59,913 | 41,472 (69.2%) | 7.5% | `SWAP3 PUSH1 ADD SWAP1 DUP2 ADD DUP1 MLOAD PUSH1 DUP1 DUP4 MSTORE SWAP3` |
| calldata-loops | @534 | 512 | 55,296 | 31,744 (57.4%) | 19.2% | `JUMPDEST DUP1 PUSH1 SHL DUP1 DUP6 ADD CALLDATALOAD SWAP1 DUP7 ADD CALLDATALOAD DUP1 DUP3 ...` |
| openzeppelin-p256 | @1327 | 175 | 52,675 | 32,375 (61.5%) | 12.6% | `POP DUP6 DUP2 DUP1 MULMOD SWAP2 DUP3 DUP8 SWAP2 MULMOD DUP7 PUSH1 DUP3 MULMOD ...` |
| solady-lib-string | @4917 | 1,536 | 49,152 | 35,328 (71.9%) | 6.1% | `JUMPDEST SWAP1 ISZERO ISZERO DUP3 DUP2 EQ PUSH2 JUMPI` |
| factorial | @110 | 6 | 47,124 | 24 (0.1%) | 77.4% | `POP PUSH0 SSTORE` |
| solady-lib-string | @5684 | 1,536 | 44,544 | 35,328 (79.3%) | 5.5% | `JUMPDEST DUP2 MLOAD PUSH1 DUP2 ISZERO PUSH2 JUMPI` |
| openzeppelin-erc20-mock | @674 | 2 | 44,506 | 96 (0.2%) | 21.4% | `DUP2 PUSH0 MSTORE PUSH1 PUSH1 MSTORE PUSH1 PUSH0 KECCAK256 DUP4 PUSH0 MSTORE PUSH1 MSTORE ...` |
| solady-lib-string | @5710 | 1,536 | 39,936 | 30,720 (76.9%) | 5.0% | `JUMPDEST DUP1 MLOAD PUSH32 AND PUSH2 JUMPI` |
| sum-array | @99 | 4 | 37,116 | 16 (0.0%) | 55.6% | `POP PUSH0 SSTORE` |
| calldata-loops | @275 | 512 | 36,352 | 23,040 (63.4%) | 12.6% | `JUMPDEST DUP1 PUSH1 SHL DUP5 ADD CALLDATALOAD DUP3 DUP2 SUB SWAP1 DUP4 LT MUL ...` |
| openzeppelin-p256 | @1273 | 175 | 36,050 | 19,250 (53.4%) | 8.6% | `JUMPDEST DUP9 DUP1 MULMOD DUP3 DUP10 DUP3 MULMOD PUSH1 DUP8 ADD MLOAD DUP5 SWAP2 ...` |
| calldata-loops | @215 | 512 | 35,840 | 22,528 (62.9%) | 12.5% | `JUMPDEST DUP1 PUSH1 SHL DUP7 ADD CALLDATALOAD DUP5 DUP2 GT SWAP1 DUP7 MUL DUP4 ...` |
| counter-loop | @167 | 1,110 | 35,520 | 28,860 (81.2%) | 27.7% | `JUMPDEST DUP1 DUP3 ADD SWAP2 DUP3 LT PUSH1 JUMPI` |
| solady-algorithms | @696 | 3,873 | 34,857 | 34,857 (100.0%) | 7.9% | `JUMPDEST SWAP4 POP SWAP3` |
| counter-loop | @104 | 6 | 34,602 | 102 (0.3%) | 27.0% | `JUMPDEST DUP1 PUSH0 SSTORE PUSH1 JUMP` |
| openzeppelin-vesting-wallet | @309 | 16 | 34,192 | 416 (1.2%) | 87.2% | `JUMPDEST SELFBALANCE PUSH1 SLOAD DUP2 ADD SWAP1 DUP2 LT PUSH1 JUMPI` |

