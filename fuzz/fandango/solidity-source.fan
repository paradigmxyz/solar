<start> ::= <source>

<source> ::= <stateless_source> | <value_source> | <mapping_source> | <fixed_array_source>

<stateless_source> ::= '/* SPDX-License-Identifier: MIT */ pragma solidity ^0.8.0; contract FandangoSource { ' <stateless_functions> ' }'

<value_source> ::= '/* SPDX-License-Identifier: MIT */ pragma solidity ^0.8.0; contract FandangoSource { uint256 public value; ' <stateless_functions> ' }'

<mapping_source> ::= '/* SPDX-License-Identifier: MIT */ pragma solidity ^0.8.0; contract FandangoSource { uint256 public value; mapping(uint256 => uint256) public values; ' <mapping_functions> ' }'

<fixed_array_source> ::= '/* SPDX-License-Identifier: MIT */ pragma solidity ^0.8.0; contract FandangoSource { uint256 public value; uint256[3] public fixedValues; ' <stateless_functions> ' }'

<stateless_functions> ::= <pure_function> | <control_function> | <loop_function> | <array_function> | <stateless_two_functions>

<mapping_functions> ::= <storage_function> | <pure_function> <storage_function> | <storage_function> <loop_function>

<stateless_two_functions> ::= <pure_function> <loop_function> | <control_function> <loop_function>

<pure_function> ::= 'function calc(uint256 a, uint256 b) external pure returns (uint256) { return ' <uint_expr> '; }'

<control_function> ::= 'function choose(uint256 a, uint256 b, bool flag) external pure returns (uint256 r) { if (' <cond> ') { r = ' <uint_expr> '; } else { r = ' <uint_expr> '; } }'

<storage_function> ::= 'function store(uint256 key, uint256 amount) external returns (uint256) { values[key] = amount; value = values[key] + 1; return value; }'

<loop_function> ::= 'function sum(uint256 n) external pure returns (uint256 acc) { for (uint256 i = 0; i < n; ++i) { acc += i; } }'

<array_function> ::= 'function at(uint256 i) external pure returns (uint256) { uint256[3] memory xs = [uint256(1), uint256(2), uint256(3)]; if (i >= 3) { return 0; } return xs[i]; }'

<uint_expr> ::= 'a' | 'b' | 'a + b' | 'a * 3 + b' | '(a & b) | 1' | 'a == 0 ? b : a' | 'a < b ? b - a : a - b'

<cond> ::= 'flag' | 'a == b' | 'a < b' | 'a != 0 && b != 0'
