//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_asm_read => 11259163
//@ run-call: Cu8_0::t_retdirect_asm_read => 11259163
//@ run-call: Cu8_0::t_ret_asm_read => 11259163
//@ run-call: Cu8_0::t_param_asm_read => 11259163
//@ run-call: Cu8_0::t_copy_asm_read => 11259163
//@ run-call: Cu8_0::t_ternary_asm_read => 11259163
//@ run-call: Cu8_0::t_ifjoin_asm_read => 11259163
//@ run-call: Cu8_0::t_loopjoin_asm_read => 11259163
//@ run-call: Cu8_0::t_tuple_asm_read => 11259163
//@ run-call: Cu8_0::t_modifier_asm_read => 11259163
//@ run-call: Cu8_0::t_ptr_asm_read => 11259163
//@ run-call: Cu8_0::t_mem_asm_read => 27
//@ run-call: Cu8_0::t_local_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_retdirect_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_ret_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_param_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_copy_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_ternary_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_ifjoin_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_loopjoin_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_tuple_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_modifier_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_ptr_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_mem_asm_read_typed_also => 28
//@ run-call: Cu64_0::t_local_asm_read => 811306251105819789623323
//@ run-call: Cu64_0::t_retdirect_asm_read => 811306251105819789623323
//@ run-call: Cu64_0::t_param_asm_read => 811306251105819789623323
//@ run-call: Cu64_0::t_mem_asm_read => 27
//@ run-call: Cu64_0::t_local_asm_read_typed_also => 811306251105819789623324
//@ run-call: Cu64_0::t_retdirect_asm_read_typed_also => 811306251105819789623324
//@ run-call: Cu64_0::t_param_asm_read_typed_also => 811306251105819789623324
//@ run-call: Cu64_0::t_mem_asm_read_typed_also => 28
//@ run-call: Ci8_0::t_local_asm_read => 11259389
//@ run-call: Ci8_0::t_retdirect_asm_read => 11259389
//@ run-call: Ci8_0::t_param_asm_read => 11259389
//@ run-call: Ci8_0::t_mem_asm_read => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_asm_read_typed_also => 11259394
//@ run-call: Ci8_0::t_retdirect_asm_read_typed_also => 11259394
//@ run-call: Ci8_0::t_param_asm_read_typed_also => 11259394
//@ run-call-fail: Ci8_0::t_mem_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: Ci24_0::t_local_asm_read => 737895514109
//@ run-call: Ci24_0::t_retdirect_asm_read => 737895514109
//@ run-call: Ci24_0::t_param_asm_read => 737895514109
//@ run-call: Ci24_0::t_mem_asm_read => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_asm_read_typed_also => 737895514114
//@ run-call: Ci24_0::t_retdirect_asm_read_typed_also => 737895514114
//@ run-call: Ci24_0::t_param_asm_read_typed_also => 737895514114
//@ run-call-fail: Ci24_0::t_mem_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: Caddr_0::t_local_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_retdirect_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_ret_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_param_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_copy_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_ternary_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_ifjoin_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_loopjoin_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_tuple_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_modifier_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_ptr_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_mem_asm_read => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_retdirect_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_ret_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_param_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_copy_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_ternary_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_ifjoin_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_loopjoin_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_tuple_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_modifier_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_ptr_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_mem_asm_read_typed_also => 103929005307130220006098923584552504982110632082
//@ run-call: Caddrp_0::t_local_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddrp_0::t_retdirect_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddrp_0::t_param_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddrp_0::t_mem_asm_read => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddrp_0::t_retdirect_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddrp_0::t_param_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddrp_0::t_mem_asm_read_typed_also => 103929005307130220006098923584552504982110632082
//@ run-call: Cctr_0::t_local_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Cctr_0::t_retdirect_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Cctr_0::t_param_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Cctr_0::t_mem_asm_read => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Cctr_0::t_retdirect_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Cctr_0::t_param_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Cctr_0::t_mem_asm_read_typed_also => 103929005307130220006098923584552504982110632082
//@ run-call: Cbool_0::t_local_asm_read => 256
//@ run-call: Cbool_0::t_retdirect_asm_read => 256
//@ run-call: Cbool_0::t_param_asm_read => 256
//@ run-call: Cbool_0::t_mem_asm_read => 1
//@ run-call: Cbool_0::t_local_asm_read_typed_also => 259
//@ run-call: Cbool_0::t_retdirect_asm_read_typed_also => 259
//@ run-call: Cbool_0::t_param_asm_read_typed_also => 259
//@ run-call: Cbool_0::t_mem_asm_read_typed_also => 4
//@ run-call: Cb1_0::t_local_asm_read => 77345497107738552411838431392522000948863935069627095510731433067785723379711
//@ run-call: Cb1_0::t_retdirect_asm_read => 77345497107738552411838431392522000948863935069627095510731433067785723379711
//@ run-call: Cb1_0::t_param_asm_read => 77345497107738552411838431392522000948863935069627095510731433067785723379711
//@ run-call: Cb1_0::t_mem_asm_read => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_asm_read_typed_also => 77345497107738552411838431392522000948863935069627095510731433067785723379714
//@ run-call: Cb1_0::t_retdirect_asm_read_typed_also => 77345497107738552411838431392522000948863935069627095510731433067785723379714
//@ run-call: Cb1_0::t_param_asm_read_typed_also => 77345497107738552411838431392522000948863935069627095510731433067785723379714
//@ run-call: Cb1_0::t_mem_asm_read_typed_also => 77345497107738552411838431392522000948863935069627095510731433067785723314179
//@ run-call: Cb4_0::t_local_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_retdirect_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_ret_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_param_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_copy_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_ternary_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_ifjoin_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_loopjoin_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_tuple_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_modifier_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_ptr_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_mem_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_retdirect_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_ret_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_param_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_copy_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_ternary_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_ifjoin_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_loopjoin_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_tuple_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_modifier_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_ptr_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_mem_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051161759749
//@ run-call: Cb20_0::t_local_asm_read => 8234104122419153896766082834368325185836758793843649141158396535749088422861
//@ run-call: Cb20_0::t_retdirect_asm_read => 8234104122419153896766082834368325185836758793843649141158396535749088422861
//@ run-call: Cb20_0::t_param_asm_read => 8234104122419153896766082834368325185836758793843649141158396535749088422861
//@ run-call: Cb20_0::t_mem_asm_read => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_asm_read_typed_also => 8234104122419153896766082834368325185836758793843649141158396535749088422866
//@ run-call: Cb20_0::t_retdirect_asm_read_typed_also => 8234104122419153896766082834368325185836758793843649141158396535749088422866
//@ run-call: Cb20_0::t_param_asm_read_typed_also => 8234104122419153896766082834368325185836758793843649141158396535749088422866
//@ run-call: Cb20_0::t_mem_asm_read_typed_also => 8234104122419153896766082834368325185836758793843649141158396535749088378885
//@ run-call: Cen_0::t_local_asm_read => 257
//@ run-call: Cen_0::t_retdirect_asm_read => 257
//@ run-call: Cen_0::t_ret_asm_read => 257
//@ run-call: Cen_0::t_param_asm_read => 257
//@ run-call: Cen_0::t_copy_asm_read => 257
//@ run-call: Cen_0::t_ternary_asm_read => 257
//@ run-call: Cen_0::t_ifjoin_asm_read => 257
//@ run-call: Cen_0::t_loopjoin_asm_read => 257
//@ run-call: Cen_0::t_tuple_asm_read => 257
//@ run-call: Cen_0::t_modifier_asm_read => 257
//@ run-call: Cen_0::t_ptr_asm_read => 257
//@ run-call-fail: Cen_0::t_mem_asm_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_asm_read => 11259163
//@ run-call: Cud_0::t_retdirect_asm_read => 11259163
//@ run-call: Cud_0::t_param_asm_read => 11259163
//@ run-call: Cud_0::t_mem_asm_read => 27
//@ run-call: Cud_0::t_local_asm_read_typed_also => 11259164
//@ run-call: Cud_0::t_retdirect_asm_read_typed_also => 11259164
//@ run-call: Cud_0::t_param_asm_read_typed_also => 11259164
//@ run-call: Cud_0::t_mem_asm_read_typed_also => 28

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: dirt surviving copies, calls, returns and joins into an assembly read.

interface IC {
    function f() external;
}

enum E {
    A,
    B,
    C
}

type U is uint8;

struct St_u8 {
    uint256 a;
    uint8 f;
}

interface IF_u8 {
    function g(uint8) external;
}

error Err_u8(uint8 v);
error Err2_u8(uint8 a, uint8 b);
error ErrS_u8(St_u8 s);

contract Callee_u8 {
    function f(uint8 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(uint8 a, uint8 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(uint8 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_u8 {
    uint256 public got;

    constructor(uint8 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_u64 {
    uint256 a;
    uint64 f;
}

interface IF_u64 {
    function g(uint64) external;
}

error Err_u64(uint64 v);
error Err2_u64(uint64 a, uint64 b);
error ErrS_u64(St_u64 s);

contract Callee_u64 {
    function f(uint64 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(uint64 a, uint64 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(uint64 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_u64 {
    uint256 public got;

    constructor(uint64 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_i8 {
    uint256 a;
    int8 f;
}

interface IF_i8 {
    function g(int8) external;
}

error Err_i8(int8 v);
error Err2_i8(int8 a, int8 b);
error ErrS_i8(St_i8 s);

contract Callee_i8 {
    function f(int8 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(int8 a, int8 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(int8 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_i8 {
    uint256 public got;

    constructor(int8 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_i24 {
    uint256 a;
    int24 f;
}

interface IF_i24 {
    function g(int24) external;
}

error Err_i24(int24 v);
error Err2_i24(int24 a, int24 b);
error ErrS_i24(St_i24 s);

contract Callee_i24 {
    function f(int24 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(int24 a, int24 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(int24 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_i24 {
    uint256 public got;

    constructor(int24 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_addr {
    uint256 a;
    address f;
}

interface IF_addr {
    function g(address) external;
}

error Err_addr(address v);
error Err2_addr(address a, address b);
error ErrS_addr(St_addr s);

contract Callee_addr {
    function f(address v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(address a, address b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(address v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_addr {
    uint256 public got;

    constructor(address v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_addrp {
    uint256 a;
    address payable f;
}

interface IF_addrp {
    function g(address payable) external;
}

error Err_addrp(address payable v);
error Err2_addrp(address payable a, address payable b);
error ErrS_addrp(St_addrp s);

contract Callee_addrp {
    function f(address payable v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(address payable a, address payable b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(address payable v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_addrp {
    uint256 public got;

    constructor(address payable v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_ctr {
    uint256 a;
    IC f;
}

interface IF_ctr {
    function g(IC) external;
}

error Err_ctr(IC v);
error Err2_ctr(IC a, IC b);
error ErrS_ctr(St_ctr s);

contract Callee_ctr {
    function f(IC v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(IC a, IC b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(IC v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_ctr {
    uint256 public got;

    constructor(IC v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_bool {
    uint256 a;
    bool f;
}

interface IF_bool {
    function g(bool) external;
}

error Err_bool(bool v);
error Err2_bool(bool a, bool b);
error ErrS_bool(St_bool s);

contract Callee_bool {
    function f(bool v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(bool a, bool b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(bool v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_bool {
    uint256 public got;

    constructor(bool v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_b1 {
    uint256 a;
    bytes1 f;
}

interface IF_b1 {
    function g(bytes1) external;
}

error Err_b1(bytes1 v);
error Err2_b1(bytes1 a, bytes1 b);
error ErrS_b1(St_b1 s);

contract Callee_b1 {
    function f(bytes1 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(bytes1 a, bytes1 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(bytes1 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_b1 {
    uint256 public got;

    constructor(bytes1 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_b4 {
    uint256 a;
    bytes4 f;
}

interface IF_b4 {
    function g(bytes4) external;
}

error Err_b4(bytes4 v);
error Err2_b4(bytes4 a, bytes4 b);
error ErrS_b4(St_b4 s);

contract Callee_b4 {
    function f(bytes4 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(bytes4 a, bytes4 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(bytes4 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_b4 {
    uint256 public got;

    constructor(bytes4 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_b20 {
    uint256 a;
    bytes20 f;
}

interface IF_b20 {
    function g(bytes20) external;
}

error Err_b20(bytes20 v);
error Err2_b20(bytes20 a, bytes20 b);
error ErrS_b20(St_b20 s);

contract Callee_b20 {
    function f(bytes20 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(bytes20 a, bytes20 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(bytes20 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_b20 {
    uint256 public got;

    constructor(bytes20 v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_en {
    uint256 a;
    E f;
}

interface IF_en {
    function g(E) external;
}

error Err_en(E v);
error Err2_en(E a, E b);
error ErrS_en(St_en s);

contract Callee_en {
    function f(E v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(E a, E b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(E v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_en {
    uint256 public got;

    constructor(E v) {
        assembly { sstore(got.slot, v) }
    }
}

struct St_ud {
    uint256 a;
    U f;
}

interface IF_ud {
    function g(U) external;
}

error Err_ud(U v);
error Err2_ud(U a, U b);
error ErrS_ud(St_ud s);

contract Callee_ud {
    function f(U v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(U a, U b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(U v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_ud {
    uint256 public got;

    constructor(U v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Cu8_0 {
    function _s_retdirect_asm_read() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_asm_read() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_asm_read(uint8 pv) internal view returns (uint256) { uint8 t = pv; uint256 r; assembly { r := t } return r; }
    uint256 _res_modifier_asm_read;
    modifier _m_modifier_asm_read(uint8 pv) { uint8 t = pv; uint256 r; assembly { r := t } _res_modifier_asm_read = r; _; }
    function _e_modifier_asm_read(uint8 rd) internal _m_modifier_asm_read(rd) returns (uint256) { return _res_modifier_asm_read; }
    function _w_ptr_asm_read() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_asm_read_typed_also() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_asm_read_typed_also() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_asm_read_typed_also(uint8 pv) internal view returns (uint256) { uint8 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    uint256 _res_modifier_asm_read_typed_also;
    modifier _m_modifier_asm_read_typed_also(uint8 pv) { uint8 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; _res_modifier_asm_read_typed_also = r2; _; }
    function _e_modifier_asm_read_typed_also(uint8 rd) internal _m_modifier_asm_read_typed_also(rd) returns (uint256) { return _res_modifier_asm_read_typed_also; }
    function _w_ptr_asm_read_typed_also() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_asm_read() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { uint8 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_ret_asm_read() external view returns (uint256) { uint8 rx = _s_ret_asm_read(); uint8 t = rx; uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_asm_read(rx); }
    function t_copy_asm_read() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_ternary_asm_read() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_ifjoin_asm_read() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_loopjoin_asm_read() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_tuple_asm_read() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_modifier_asm_read() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_asm_read(rx); }
    function t_ptr_asm_read() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { uint8 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ret_asm_read_typed_also() external view returns (uint256) { uint8 rx = _s_ret_asm_read_typed_also(); uint8 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_asm_read_typed_also(rx); }
    function t_copy_asm_read_typed_also() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ternary_asm_read_typed_also() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ifjoin_asm_read_typed_also() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_loopjoin_asm_read_typed_also() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_tuple_asm_read_typed_also() external view returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_modifier_asm_read_typed_also() external returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_asm_read_typed_also(rx); }
    function t_ptr_asm_read_typed_also() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read_typed_also; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_mem_asm_read_typed_also() external view returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cu64_0 {
    function _s_retdirect_asm_read() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_asm_read(uint64 pv) internal view returns (uint256) { uint64 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_asm_read_typed_also(uint64 pv) internal view returns (uint256) { uint64 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { uint64 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_asm_read(rx); }
    function t_mem_asm_read() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64 t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { uint64 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_asm_read_typed_also(rx); }
    function t_mem_asm_read_typed_also() external view returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Ci8_0 {
    function _s_retdirect_asm_read() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_asm_read(int8 pv) internal view returns (uint256) { int8 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_asm_read_typed_also(int8 pv) internal view returns (uint256) { int8 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { int8 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_asm_read(rx); }
    function t_mem_asm_read() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { int8 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_asm_read_typed_also(rx); }
    function t_mem_asm_read_typed_also() external view returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Ci24_0 {
    function _s_retdirect_asm_read() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_asm_read(int24 pv) internal view returns (uint256) { int24 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_asm_read_typed_also(int24 pv) internal view returns (uint256) { int24 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { int24 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_asm_read(rx); }
    function t_mem_asm_read() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24 t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { int24 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_asm_read_typed_also(rx); }
    function t_mem_asm_read_typed_also() external view returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Caddr_0 {
    function _s_retdirect_asm_read() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_asm_read() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read(address pv) internal view returns (uint256) { address t = pv; uint256 r; assembly { r := t } return r; }
    uint256 _res_modifier_asm_read;
    modifier _m_modifier_asm_read(address pv) { address t = pv; uint256 r; assembly { r := t } _res_modifier_asm_read = r; _; }
    function _e_modifier_asm_read(address rd) internal _m_modifier_asm_read(rd) returns (uint256) { return _res_modifier_asm_read; }
    function _w_ptr_asm_read() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_asm_read_typed_also() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_asm_read_typed_also() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read_typed_also(address pv) internal view returns (uint256) { address t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    uint256 _res_modifier_asm_read_typed_also;
    modifier _m_modifier_asm_read_typed_also(address pv) { address t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; _res_modifier_asm_read_typed_also = r2; _; }
    function _e_modifier_asm_read_typed_also(address rd) internal _m_modifier_asm_read_typed_also(rd) returns (uint256) { return _res_modifier_asm_read_typed_also; }
    function _w_ptr_asm_read_typed_also() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_asm_read() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { address t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_ret_asm_read() external view returns (uint256) { address rx = _s_ret_asm_read(); address t = rx; uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read(rx); }
    function t_copy_asm_read() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; address t = ry; uint256 r; assembly { r := t } return r; }
    function t_ternary_asm_read() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; address t = ry; uint256 r; assembly { r := t } return r; }
    function t_ifjoin_asm_read() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } address t = ry; uint256 r; assembly { r := t } return r; }
    function t_loopjoin_asm_read() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } address t = ry; uint256 r; assembly { r := t } return r; }
    function t_tuple_asm_read() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; address t = ry; uint256 r; assembly { r := t } return r; }
    function t_modifier_asm_read() external returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_asm_read(rx); }
    function t_ptr_asm_read() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); address t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { address t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ret_asm_read_typed_also() external view returns (uint256) { address rx = _s_ret_asm_read_typed_also(); address t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read_typed_also(rx); }
    function t_copy_asm_read_typed_also() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ternary_asm_read_typed_also() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ifjoin_asm_read_typed_also() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_loopjoin_asm_read_typed_also() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_tuple_asm_read_typed_also() external view returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_modifier_asm_read_typed_also() external returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_asm_read_typed_also(rx); }
    function t_ptr_asm_read_typed_also() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read_typed_also; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_mem_asm_read_typed_also() external view returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Caddrp_0 {
    function _s_retdirect_asm_read() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read(address payable pv) internal view returns (uint256) { address payable t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read_typed_also(address payable pv) internal view returns (uint256) { address payable t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { address payable t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read(rx); }
    function t_mem_asm_read() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { address payable t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read_typed_also(rx); }
    function t_mem_asm_read_typed_also() external view returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cctr_0 {
    function _s_retdirect_asm_read() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read(IC pv) internal view returns (uint256) { IC t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read_typed_also(IC pv) internal view returns (uint256) { IC t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { IC t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read(rx); }
    function t_mem_asm_read() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { IC t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read_typed_also(rx); }
    function t_mem_asm_read_typed_also() external view returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cbool_0 {
    function _s_retdirect_asm_read() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_asm_read(bool pv) internal view returns (uint256) { bool t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_asm_read_typed_also(bool pv) internal view returns (uint256) { bool t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view returns (uint256) { bool rx; assembly { rx := 0x100 } bool t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { bool t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_asm_read(rx); }
    function t_mem_asm_read() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { bool rx; assembly { rx := 0x100 } bool t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { bool t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_asm_read_typed_also(rx); }
    function t_mem_asm_read_typed_also() external view returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cb1_0 {
    function _s_retdirect_asm_read() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_asm_read(bytes1 pv) internal view returns (uint256) { bytes1 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_asm_read_typed_also(bytes1 pv) internal view returns (uint256) { bytes1 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { bytes1 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_asm_read(rx); }
    function t_mem_asm_read() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1 t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { bytes1 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_asm_read_typed_also(rx); }
    function t_mem_asm_read_typed_also() external view returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cb4_0 {
    function _s_retdirect_asm_read() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_asm_read() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_asm_read(bytes4 pv) internal view returns (uint256) { bytes4 t = pv; uint256 r; assembly { r := t } return r; }
    uint256 _res_modifier_asm_read;
    modifier _m_modifier_asm_read(bytes4 pv) { bytes4 t = pv; uint256 r; assembly { r := t } _res_modifier_asm_read = r; _; }
    function _e_modifier_asm_read(bytes4 rd) internal _m_modifier_asm_read(rd) returns (uint256) { return _res_modifier_asm_read; }
    function _w_ptr_asm_read() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_asm_read_typed_also() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_asm_read_typed_also() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_asm_read_typed_also(bytes4 pv) internal view returns (uint256) { bytes4 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    uint256 _res_modifier_asm_read_typed_also;
    modifier _m_modifier_asm_read_typed_also(bytes4 pv) { bytes4 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; _res_modifier_asm_read_typed_also = r2; _; }
    function _e_modifier_asm_read_typed_also(bytes4 rd) internal _m_modifier_asm_read_typed_also(rd) returns (uint256) { return _res_modifier_asm_read_typed_also; }
    function _w_ptr_asm_read_typed_also() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_local_asm_read() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { bytes4 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_ret_asm_read() external view returns (uint256) { bytes4 rx = _s_ret_asm_read(); bytes4 t = rx; uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_asm_read(rx); }
    function t_copy_asm_read() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_ternary_asm_read() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_ifjoin_asm_read() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_loopjoin_asm_read() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_tuple_asm_read() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_modifier_asm_read() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_asm_read(rx); }
    function t_ptr_asm_read() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { bytes4 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ret_asm_read_typed_also() external view returns (uint256) { bytes4 rx = _s_ret_asm_read_typed_also(); bytes4 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_asm_read_typed_also(rx); }
    function t_copy_asm_read_typed_also() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ternary_asm_read_typed_also() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ifjoin_asm_read_typed_also() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_loopjoin_asm_read_typed_also() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_tuple_asm_read_typed_also() external view returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_modifier_asm_read_typed_also() external returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_asm_read_typed_also(rx); }
    function t_ptr_asm_read_typed_also() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read_typed_also; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_mem_asm_read_typed_also() external view returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cb20_0 {
    function _s_retdirect_asm_read() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_asm_read(bytes20 pv) internal view returns (uint256) { bytes20 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_asm_read_typed_also(bytes20 pv) internal view returns (uint256) { bytes20 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { bytes20 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_asm_read(rx); }
    function t_mem_asm_read() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20 t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { bytes20 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_asm_read_typed_also(rx); }
    function t_mem_asm_read_typed_also() external view returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cen_0 {
    function _s_retdirect_asm_read() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_asm_read() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_asm_read(E pv) internal view returns (uint256) { E t = pv; uint256 r; assembly { r := t } return r; }
    uint256 _res_modifier_asm_read;
    modifier _m_modifier_asm_read(E pv) { E t = pv; uint256 r; assembly { r := t } _res_modifier_asm_read = r; _; }
    function _e_modifier_asm_read(E rd) internal _m_modifier_asm_read(rd) returns (uint256) { return _res_modifier_asm_read; }
    function _w_ptr_asm_read() internal pure returns (uint256) { return 0x101; }
    function _s_retdirect_asm_read_typed_also() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_asm_read_typed_also() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_asm_read_typed_also(E pv) internal view returns (uint256) { E t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    uint256 _res_modifier_asm_read_typed_also;
    modifier _m_modifier_asm_read_typed_also(E pv) { E t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; _res_modifier_asm_read_typed_also = r2; _; }
    function _e_modifier_asm_read_typed_also(E rd) internal _m_modifier_asm_read_typed_also(rd) returns (uint256) { return _res_modifier_asm_read_typed_also; }
    function _w_ptr_asm_read_typed_also() internal pure returns (uint256) { return 0x101; }
    function t_local_asm_read() external view returns (uint256) { E rx; assembly { rx := 0x101 } E t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { E t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_ret_asm_read() external view returns (uint256) { E rx = _s_ret_asm_read(); E t = rx; uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_asm_read(rx); }
    function t_copy_asm_read() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; E t = ry; uint256 r; assembly { r := t } return r; }
    function t_ternary_asm_read() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; E t = ry; uint256 r; assembly { r := t } return r; }
    function t_ifjoin_asm_read() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } E t = ry; uint256 r; assembly { r := t } return r; }
    function t_loopjoin_asm_read() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } E t = ry; uint256 r; assembly { r := t } return r; }
    function t_tuple_asm_read() external view returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; E t = ry; uint256 r; assembly { r := t } return r; }
    function t_modifier_asm_read() external returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_asm_read(rx); }
    function t_ptr_asm_read() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); E t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { E rx; assembly { rx := 0x101 } E t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { E t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ret_asm_read_typed_also() external view returns (uint256) { E rx = _s_ret_asm_read_typed_also(); E t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_asm_read_typed_also(rx); }
    function t_copy_asm_read_typed_also() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ternary_asm_read_typed_also() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ifjoin_asm_read_typed_also() external view returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_loopjoin_asm_read_typed_also() external view returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_tuple_asm_read_typed_also() external view returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_modifier_asm_read_typed_also() external returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_asm_read_typed_also(rx); }
    function t_ptr_asm_read_typed_also() external view returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read_typed_also; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_mem_asm_read_typed_also() external view returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cud_0 {
    function _s_retdirect_asm_read() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_asm_read(U pv) internal view returns (uint256) { U t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_asm_read_typed_also(U pv) internal view returns (uint256) { U t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } U t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view returns (uint256) { U t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_asm_read(rx); }
    function t_mem_asm_read() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } U t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view returns (uint256) { U t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_asm_read_typed_also(rx); }
    function t_mem_asm_read_typed_also() external view returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
