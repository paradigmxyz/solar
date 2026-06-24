<start> ::= <dynamic> | <numeric_fixed> | <arrays> | <panic_div> | <panic_sub> | <array_at> | <set_value> | <get_value> | <add_value> | <set_blob> | <blob_hash>

<dynamic> ::= '{"label":"dynamic","mode":"call","signature":"f(uint256,bool,bytes,string)","args":[' <uint256> ',' <bool> ',' <bytes> ',' <string> ']}'

<numeric_fixed> ::= '{"label":"numeric-fixed","mode":"call","signature":"numericFixed(int8,int256,bytes1,bytes31,bytes32,address)","args":[' <int8> ',' <int256> ',' <bytes1> ',' <bytes31> ',' <bytes32> ',' <address> ']}'

<arrays> ::= '{"label":"arrays","mode":"call","signature":"arrays(uint256[],uint256[3])","args":[' <uint_array> ',' <uint3_array> ']}'

<panic_div> ::= '{"label":"panic-div","mode":"call","signature":"panicDiv(uint256,uint256)","args":[' <uint256> ',' <uint_divisor> ']}'

<panic_sub> ::= '{"label":"panic-sub","mode":"call","signature":"panicSub(uint256,uint256)","args":[' <uint_small> ',' <uint256> ']}'

<array_at> ::= '{"label":"array-at","mode":"call","signature":"arrayAt(uint256[],uint256)","args":[' <uint_array> ',' <array_index> ']}'

<set_value> ::= '{"label":"state-set","mode":"tx","signature":"setValue(uint256,uint256)","args":[' <uint_key> ',' <uint256> ']}'

<get_value> ::= '{"label":"state-get","mode":"call","signature":"getValue(uint256)","args":[' <uint_key> ']}'

<add_value> ::= '{"label":"state-add","mode":"tx","signature":"addValue(uint256,uint256)","args":[' <uint_key> ',' <uint_small> ']}'

<set_blob> ::= '{"label":"state-blob-set","mode":"tx","signature":"setBlob(bytes)","args":[' <bytes> ']}'

<blob_hash> ::= '{"label":"state-blob-hash","mode":"call","signature":"blobHash()","args":[]}'

<uint256> ::= <uint_small> | <uint_max>

<uint_small> ::= '"0"' | '"1"' | '"31"' | '"32"' | '"33"'

<uint_max> ::= '"115792089237316195423570985008687907853269984665640564039457584007913129639935"'

<uint_divisor> ::= '"0"' | '"1"' | '"2"' | '"33"'

<uint_key> ::= '"0"' | '"1"' | '"2"' | '"31"'

<array_index> ::= '"0"' | '"1"' | '"2"' | '"3"' | '"33"'

<bool> ::= 'true' | 'false'

<int8> ::= '"-128"' | '"-1"' | '"0"' | '"1"' | '"127"'

<int256> ::= '"-57896044618658097711785492504343953926634992332820282019728792003956564819968"' | '"-1"' | '"0"' | '"1"' | '"57896044618658097711785492504343953926634992332820282019728792003956564819967"'

<address> ::= '"0x0000000000000000000000000000000000000000"' | '"0x0000000000000000000000000000000000000001"' | '"0xffffffffffffffffffffffffffffffffffffffff"'

<bytes> ::= <bytes_empty> | <bytes_one> | <bytes_31> | <bytes_32> | <bytes_33>

<bytes_empty> ::= '"0x"'
<bytes_one> ::= '"0x00"'
<bytes_31> ::= '"0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"'
<bytes_32> ::= '"0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"'
<bytes_33> ::= '"0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021"'

<bytes1> ::= '"0x00"' | '"0xff"'

<bytes31> ::= <bytes_31>

<bytes32> ::= <bytes_32>

<string> ::= '""' | '"a"' | '"hello"' | '"thirty-one-byte-string-value!!"'

<uint_array> ::= '[]' | '["0"]' | '["1","2"]' | '["31","32","33"]'

<uint3_array> ::= '["0","0","0"]' | '["1","2","3"]' | '["31","32","33"]'
