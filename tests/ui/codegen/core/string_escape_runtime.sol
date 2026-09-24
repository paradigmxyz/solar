//@ codegen-matrix: standard portable gasmir
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[gasmir] compile-flags: -Ogas -Zdump=mir
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[gasmir] normalize-stdout-test: "(?s).+" -> ""
//@[mir] filecheck:
//@[gasmir] filecheck: --check-prefix=GAS
//@ run-call: html 0x => 0x
//@ run-call: html 0x706c61696e => 0x706c61696e
//@ run-call: html 0x3c6120687265663d2278223e546f6d2026204a6572727927733c2f613e => 0x266c743b6120687265663d2671756f743b782671756f743b2667743b546f6d2026616d703b204a65727279262333393b73266c743b2f612667743b
//@ run-call: html 0xc3a93c3e => 0xc3a9266c743b2667743b
//@ run-call: html 0x262626262626262626262626262626262626262626262626262626262626262626 => 0x26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b26616d703b
//@ run-call: html 0x616161616161616161616161616161616161616161616161616161616161613c6262626262626262 => 0x61616161616161616161616161616161616161616161616161616161616161266c743b6262626262626262
//@ run-call: html 0x78787878787878787878787878787878787878787878787878787878787878782727797979797979797979797979797979797979797979797979797979797979 => 0x7878787878787878787878787878787878787878787878787878787878787878262333393b262333393b797979797979797979797979797979797979797979797979797979797979
//@ run-call: html 0x71717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171 => 0x71717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171717171
//@ run-call: json 0x, false => 0x
//@ run-call: json 0x, true => 0x2222
//@ run-call: json 0x706c61696e, false => 0x706c61696e
//@ run-call: json 0x706c61696e, true => 0x22706c61696e22
//@ run-call: json 0x73617920226869225c, false => 0x736179205c2268695c225c5c
//@ run-call: json 0x73617920226869225c, true => 0x22736179205c2268695c225c5c22
//@ run-call: json 0x746162096e65770a6c696e65, false => 0x7461625c746e65775c6e6c696e65
//@ run-call: json 0x746162096e65770a6c696e65, true => 0x227461625c746e65775c6e6c696e6522
//@ run-call: json 0x010b1f, false => 0x5c75303030315c75303030625c7530303166
//@ run-call: json 0x010b1f, true => 0x225c75303030315c75303030625c753030316622
//@ run-call: json 0xc3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9, false => 0xc3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9
//@ run-call: json 0xc3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9, true => 0x22c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a922
//@ run-call: json 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f7f, false => 0x5c75303030305c75303030315c75303030325c75303030335c75303030345c75303030355c75303030365c75303030375c625c745c6e5c75303030625c665c725c75303030655c75303030665c75303031305c75303031315c75303031325c75303031335c75303031345c75303031355c75303031365c75303031375c75303031385c75303031395c75303031615c75303031625c75303031635c75303031645c75303031655c75303031667f
//@ run-call: json 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f7f, true => 0x225c75303030305c75303030315c75303030325c75303030335c75303030345c75303030355c75303030365c75303030375c625c745c6e5c75303030625c665c725c75303030655c75303030665c75303031305c75303031315c75303031325c75303031335c75303031345c75303031355c75303031365c75303031375c75303031385c75303031395c75303031615c75303031625c75303031635c75303031645c75303031655c75303031667f22
//@ run-call: json 0x7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a227a7a7a7a7a7a7a7a7a7a, false => 0x7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a5c227a7a7a7a7a7a7a7a7a7a
//@ run-call: json 0x7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a227a7a7a7a7a7a7a7a7a7a, true => 0x227a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a5c227a7a7a7a7a7a7a7a7a7a22
//@ run-call: jsonBare 0x => 0x
//@ run-call: jsonBare 0x706c61696e => 0x706c61696e
//@ run-call: jsonBare 0x73617920226869225c => 0x736179205c2268695c225c5c
//@ run-call: jsonBare 0x746162096e65770a6c696e65 => 0x7461625c746e65775c6e6c696e65
//@ run-call: uri 0x => 0x
//@ run-call: uri 0x6162632d5f2e217e2a272829 => 0x6162632d5f2e217e2a272829
//@ run-call: uri 0x61206226633d642f653f66 => 0x612532306225323663253344642532466525334666
//@ run-call: uri 0xc3a9e697a5e69cac => 0x254333254139254536253937254135254536253943254143
//@ run-call: uri 0x252525252525252525252525252525252525252525252525252525252525252525 => 0x253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235253235
//@ run-call: uri 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f => 0x253030253031253032253033253034253035253036253037253038253039253041253042253043253044253045253046253130253131253132253133253134253135253136253137253138253139253141253142253143253144253145253146253230212532322532332532342532352532362728292a2532422532432d2e253246303132333435363738392533412533422533432533442533452533462534304142434445464748494a4b4c4d4e4f
//@ run-call: uri 0x41414141414141414141414141414141414141414141414141414141414141414141414141414141 => 0x41414141414141414141414141414141414141414141414141414141414141414141414141414141
//@ run-call: sweep 0; gas=16000000 => true
//@ run-call: sweep 1; gas=16000000 => true
//@ run-call: sweep 2; gas=16000000 => true
//@ run-call: sweep 3; gas=16000000 => true
//@ run-call: sweep 5; gas=16000000 => true
//@ run-call: sweep 8; gas=16000000 => true
//@ run-call: sweep 31; gas=16000000 => true
//@ run-call: sweep 32; gas=16000000 => true
//@ run-call: sweep 33; gas=16000000 => true
//@ run-call: sweep 34; gas=16000000 => true
//@ run-call: sweep 40; gas=16000000 => true
//@ run-call: sweep 63; gas=16000000 => true
//@ run-call: sweep 64; gas=16000000 => true
//@ run-call: sweep 65; gas=16000000 => true
//@ run-call: sweep 70; gas=16000000 => true
//@ run-call: sweep 100; gas=16000000 => true

import {Strings} from "solar:core/v1/Strings.sol";

contract Test {
    // CHECK-LABEL: fn @html{{[( ]}}
    // CHECK: icall @core_string_escape_html,
    function html(bytes memory s) public pure returns (bytes memory) {
        return bytes(Strings.escapeHTML(string(s)));
    }

    // CHECK-LABEL: fn @json{{[( ]}}
    // CHECK: icall @core_string_escape_json_quotable,
    function json(bytes memory s, bool quotes) public pure returns (bytes memory) {
        return bytes(Strings.escapeJSON(string(s), quotes));
    }

    // Gas builds give the unquoted form its own body; other builds share the
    // quoted one with a false flag.
    // CHECK-LABEL: fn @jsonBare{{[( ]}}
    // CHECK: icall @core_string_escape_json_quotable, arg0, 0
    // GAS-LABEL: fn @jsonBare{{[( ]}}
    // GAS: icall @core_string_escape_json,
    function jsonBare(bytes memory s) public pure returns (bytes memory) {
        return bytes(Strings.escapeJSON(string(s)));
    }

    // CHECK-LABEL: fn @uri{{[( ]}}
    // CHECK: icall @core_string_uri_component,
    function uri(bytes memory s) public pure returns (bytes memory) {
        return bytes(Strings.encodeURIComponent(string(s)));
    }

    // Bytes drawn mostly from the ones each encoding rewrites, compared with
    // direct per-byte encodings.
    function sweep(uint256 length) public pure returns (bool) {
        bytes memory specials = "\"&'<>\\\x00\x01\x08\x09\x0a\x0c\x0d\x1f\x7f\x80\xff %/~aZ09";
        for (uint256 seed; seed < 8; ++seed) {
            bytes memory s = new bytes(length);
            uint256 x = uint256(keccak256(abi.encode(seed, length)));
            for (uint256 i; i < length; ++i) {
                x = uint256(keccak256(abi.encode(x)));
                s[i] = x % 3 == 0 ? specials[(x >> 8) % specials.length] : bytes1(uint8(x >> 16));
            }
            string memory text = string(s);
            if (keccak256(bytes(Strings.escapeHTML(text))) != keccak256(_html(s))) return false;
            if (keccak256(bytes(Strings.escapeJSON(text, seed % 2 == 1))) != keccak256(_json(s, seed % 2 == 1))) {
                return false;
            }
            if (keccak256(bytes(Strings.encodeURIComponent(text))) != keccak256(_uri(s))) return false;
        }
        return true;
    }

    function _html(bytes memory s) private pure returns (bytes memory out) {
        for (uint256 i; i < s.length; ++i) {
            bytes1 c = s[i];
            if (c == '"') out = bytes.concat(out, "&quot;");
            else if (c == "&") out = bytes.concat(out, "&amp;");
            else if (c == "'") out = bytes.concat(out, "&#39;");
            else if (c == "<") out = bytes.concat(out, "&lt;");
            else if (c == ">") out = bytes.concat(out, "&gt;");
            else out = bytes.concat(out, c);
        }
    }

    function _json(bytes memory s, bool quotes) private pure returns (bytes memory out) {
        bytes memory digits = "0123456789abcdef";
        if (quotes) out = '"';
        for (uint256 i; i < s.length; ++i) {
            bytes1 c = s[i];
            uint8 v = uint8(c);
            if (c == '"' || c == "\\") out = bytes.concat(out, "\\", c);
            else if (v >= 0x20) out = bytes.concat(out, c);
            else if (v == 8) out = bytes.concat(out, "\\b");
            else if (v == 9) out = bytes.concat(out, "\\t");
            else if (v == 10) out = bytes.concat(out, "\\n");
            else if (v == 12) out = bytes.concat(out, "\\f");
            else if (v == 13) out = bytes.concat(out, "\\r");
            else out = bytes.concat(out, "\\u00", digits[v >> 4], digits[v & 15]);
        }
        if (quotes) out = bytes.concat(out, '"');
    }

    function _uri(bytes memory s) private pure returns (bytes memory out) {
        bytes memory digits = "0123456789ABCDEF";
        for (uint256 i; i < s.length; ++i) {
            uint8 v = uint8(s[i]);
            bool keep = (v >= 0x30 && v <= 0x39) || (v >= 0x41 && v <= 0x5a) || (v >= 0x61 && v <= 0x7a)
                || v == 0x2d || v == 0x5f || v == 0x2e || v == 0x21 || v == 0x7e || v == 0x2a
                || v == 0x27 || v == 0x28 || v == 0x29;
            if (keep) out = bytes.concat(out, s[i]);
            else out = bytes.concat(out, "%", digits[v >> 4], digits[v & 15]);
        }
    }
}
