<start> ::= <source>

<source> ::= <prefix> <body> <suffix> <trailing_newline>

<trailing_newline> ::= '' | '\n'

<prefix> ::= '/* SPDX-License-Identifier: MIT */ pragma solidity ^0.8.0; contract FandangoRuntime { event Seen(uint256 indexed tag, uint256 value); uint256 public value; mapping(uint256 => uint256) public values; function setup(uint256 seed) external { unchecked { value = seed & 1023; values[seed & 7] = seed + 1; emit Seen(0, value); } } function observe(uint256 key) external view returns (uint256, uint256) { return (value, values[key & 7]); } function helper(uint256 x) internal pure returns (uint256) { unchecked { return (x * 7) ^ 3; } } function mix(uint256 x, uint256 y) internal pure returns (uint256) { unchecked { return (x ^ y) + (x & 15); } } function run(uint256 a, uint256 b, bytes calldata data) external returns (uint256 r) { unchecked { '

<suffix> ::= ' value = r; values[a & 7] = r; emit Seen(1, r); return r; } } }'

<body> ::= <arith_body> | <branch_body> | <loop_body> | <mapping_body> | <array_body> | <bytes_body> | <combined_body>

<arith_body> ::= 'r = a + b + value + helper(a & 15);' | 'r = mix(a, b) + value + helper(b & 7);' | 'r = ((a | 1) * 3) + (b & 255) + value;' | 'r = (a ^ (b << 1)) + helper(value & 31);'

<branch_body> ::= 'if ((a ^ b) & 1 == 0) { r = helper(a) + value; } else { r = helper(b) + values[b & 7]; }' | 'if (a < b) { r = b - a + value; } else { r = a - b + values[a & 7]; }' | 'if (data.length == 0) { r = helper(a); } else { r = helper(b) + uint8(data[0]); }' | 'if ((a & 3) == 0) { r = mix(a, value); } else if ((b & 3) == 0) { r = mix(b, value); } else { r = mix(a, b); }'

<loop_body> ::= 'uint256 limit = a & 7; r = value; for (uint256 i = 0; i < limit; ++i) { r += i + b; }' | 'uint256 limit = (b & 3) + 1; r = a; for (uint256 i = 0; i < limit; ++i) { r = mix(r, i); }' | 'uint256 limit = data.length < 5 ? data.length : 5; r = value; for (uint256 i = 0; i < limit; ++i) { r += uint8(data[i]); }' | 'uint256 i = 0; r = b; while (i < (a & 3)) { r += helper(i); ++i; }'

<mapping_body> ::= 'uint256 key = (a + b) & 7; r = values[key] + helper(key); values[key] = r + 1;' | 'uint256 key = a & 7; values[key] = values[key] + b + 1; r = values[key] + value;' | 'uint256 key = b & 7; r = values[key] ^ helper(a); values[(key + 1) & 7] = r;' | 'uint256 key = (value + a) & 7; r = values[key] + values[(key + 1) & 7] + b;'

<array_body> ::= 'uint256[3] memory xs = [uint256(1), uint256(2), uint256(3)]; r = xs[a % 3] + helper(b) + value;' | 'uint256[4] memory xs = [uint256(5), uint256(8), uint256(13), uint256(21)]; r = xs[a & 3] + xs[b & 3] + value;' | 'uint256[2] memory xs; xs[0] = a + 1; xs[1] = b + 2; r = xs[(a ^ b) & 1] + value;' | 'uint256[3] memory xs = [a, b, value]; r = xs[0] + xs[1] + xs[2];'

<bytes_body> ::= 'r = value + a; if (data.length != 0) { r += uint8(data[0]); } if (data.length > 2) { r += uint8(data[2]) * 3; }' | 'r = b; if (data.length > 1) { r = mix(r, uint8(data[1])); }' | 'uint256 limit = data.length < 4 ? data.length : 4; r = value; for (uint256 i = 0; i < limit; ++i) { r = (r << 1) + uint8(data[i]); }' | 'r = data.length; if (data.length > 0) { r += uint8(data[data.length - 1]); }'

<combined_body> ::= 'uint256 key = a & 7; uint256[2] memory xs = [values[key], helper(b)]; r = xs[0] + xs[1] + value;' | 'uint256 key = b & 7; if (values[key] == 0) { values[key] = helper(a); } r = values[key] + mix(a, b);' | 'uint256 limit = (a & 3) + 1; r = values[b & 7]; for (uint256 i = 0; i < limit; ++i) { r += mix(i, value); }' | 'r = mix(a, b); if (data.length > 0) { values[uint8(data[0]) & 7] = r; } r += values[a & 7];'
