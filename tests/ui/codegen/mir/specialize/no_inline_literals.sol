//@ codegen-matrix: standard gasmir sizemir
//@[gasmir] compile-flags: -Ogas -Zdump=mir
//@[sizemir] compile-flags: -Osize -Zdump=mir
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[gasmir] normalize-stdout-test: "(?s).+" -> ""
//@[sizemir] normalize-stdout-test: "(?s).+" -> ""
//@[gasmir] filecheck: --check-prefix=GAS
//@[sizemir] filecheck: --check-prefix=SIZE
//@ run-call: encode 0x666f6f => "Zm9v"
//@ run-call: encode 0x666f6f62 => "Zm9vYg=="

import {Base64} from "solar:core/v1/codecs/Base64.sol";

// The shared Base64 encoder stays one no-inline body, and its only caller
// passes literal flags. Size builds substitute them in place, which clones
// nothing, and the parameters disappear; gas builds keep the helper as it is.
contract C {
    // GAS: icall @core_base64_encode, {{v[0-9]+}}, false, false
    // GAS: fn @core_base64_encode(arg0: memptr, arg1: i1, arg2: i1)
    // SIZE: icall @core_base64_encode, {{v[0-9]+}}{{$}}
    // SIZE: fn @core_base64_encode(arg0: memptr) ->
    function encode(bytes memory data) public pure returns (string memory) {
        return Base64.encode(data);
    }
}
