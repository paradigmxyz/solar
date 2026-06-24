<start> ::= '{"signature":"f(uint256,bool,bytes,string)","args":[' <uint256> ',' <bool> ',' <bytes> ',' <string> ']}'

<uint256> ::= <uint_small> | <uint_max>

<uint_small> ::= '"0"' | '"1"' | '"31"' | '"32"' | '"33"'

<uint_max> ::= '"115792089237316195423570985008687907853269984665640564039457584007913129639935"'

<bool> ::= 'true' | 'false'

<bytes> ::= <bytes_empty> | <bytes_one> | <bytes_31> | <bytes_32> | <bytes_33>

<bytes_empty> ::= '"0x"'
<bytes_one> ::= '"0x00"'
<bytes_31> ::= '"0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"'
<bytes_32> ::= '"0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"'
<bytes_33> ::= '"0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021"'

<string> ::= '""' | '"a"' | '"hello"' | '"thirty-one-byte-string-value!!"'
