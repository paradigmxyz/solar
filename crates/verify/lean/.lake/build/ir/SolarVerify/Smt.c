// Lean compiler output
// Module: SolarVerify.Smt
// Imports: public import Init public meta import Init
#include <lean/lean.h>
#if defined(__clang__)
#pragma clang diagnostic ignored "-Wunused-parameter"
#pragma clang diagnostic ignored "-Wunused-label"
#elif defined(__GNUC__) && !defined(__CLANG__)
#pragma GCC diagnostic ignored "-Wunused-parameter"
#pragma GCC diagnostic ignored "-Wunused-label"
#pragma GCC diagnostic ignored "-Wunused-but-set-variable"
#endif
#ifdef __cplusplus
extern "C" {
#endif
uint8_t lean_nat_dec_eq(lean_object*, lean_object*);
lean_object* l_Repr_addAppParen(lean_object*, lean_object*);
uint8_t lean_nat_dec_le(lean_object*, lean_object*);
lean_object* lean_nat_to_int(lean_object*);
lean_object* l_Nat_reprFast(lean_object*);
lean_object* lean_string_length(lean_object*);
lean_object* lean_nat_sub(lean_object*, lean_object*);
lean_object* l_Bool_repr___redArg(uint8_t);
uint8_t lean_int_dec_lt(lean_object*, lean_object*);
lean_object* l_Int_repr(lean_object*);
lean_object* l_BitVec_ofNat(lean_object*, lean_object*);
lean_object* l_String_quote(lean_object*);
uint8_t lean_int_dec_eq(lean_object*, lean_object*);
lean_object* lean_int_ediv(lean_object*, lean_object*);
lean_object* l_List_reverse___redArg(lean_object*);
lean_object* l_List_appendTR___redArg(lean_object*, lean_object*);
lean_object* lean_int_emod(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_ctorIdx(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_ctorIdx___boxed(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_ctorElim(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___boxed(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_bool_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_bool_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_int_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_int_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_bv_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_bv_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_array_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_array_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_tup_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_tup_elim(lean_object*, lean_object*, lean_object*, lean_object*);
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 24, .m_capacity = 24, .m_length = 23, .m_data = "SolarVerify.Smt.Srt.int"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__0_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__1_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__0_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__1 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__1_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__2_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Srt.bool"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__2 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__2_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__3_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__2_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__3 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__3_value;
static lean_once_cell_t lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4;
static lean_once_cell_t lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__6_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Srt.bv"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__6 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__6_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__7_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__6_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__7 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__7_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__8_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__7_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__8 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__8_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__9_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Srt.array"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__9 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__9_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__10_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__9_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__10 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__10_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__11_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__10_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__11 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__11_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__12_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 24, .m_capacity = 24, .m_length = 23, .m_data = "SolarVerify.Smt.Srt.tup"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__12 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__12_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__13_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__12_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__13 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__13_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__14_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__13_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__14 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__14_value;
static const lean_string_object lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 3, .m_capacity = 3, .m_length = 2, .m_data = "[]"};
static const lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__0 = (const lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__0_value;
static const lean_ctor_object lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__1_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__0_value)}};
static const lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__1 = (const lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__1_value;
static const lean_string_object lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__3_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 2, .m_capacity = 2, .m_length = 1, .m_data = ","};
static const lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__3 = (const lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__3_value;
static const lean_ctor_object lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__4_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__3_value)}};
static const lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__4 = (const lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__4_value;
static const lean_ctor_object lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__5_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__4_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__5 = (const lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__5_value;
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0_spec__1_spec__2(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0_spec__1(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0(lean_object*, lean_object*);
static const lean_string_object lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__2_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 2, .m_capacity = 2, .m_length = 1, .m_data = "["};
static const lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__2 = (const lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__2_value;
static lean_once_cell_t lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__7_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__7;
static lean_once_cell_t lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8;
static const lean_ctor_object lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__9_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__2_value)}};
static const lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__9 = (const lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__9_value;
static const lean_string_object lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__6_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 2, .m_capacity = 2, .m_length = 1, .m_data = "]"};
static const lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__6 = (const lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__6_value;
static const lean_ctor_object lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__10_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__6_value)}};
static const lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__10 = (const lean_object*)&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__10_value;
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0___lam__0(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___boxed(lean_object*, lean_object*);
static const lean_closure_object lp_solar__verify_SolarVerify_Smt_instReprSrt___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_closure_object) + sizeof(void*)*0, .m_other = 0, .m_tag = 245}, .m_fun = (void*)lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___boxed, .m_arity = 2, .m_num_fixed = 0, .m_objs = {} };
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt___closed__0_value;
LEAN_EXPORT const lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprSrt___closed__0_value;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedSrt_default;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedSrt;
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_Srt_decEq(lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_Srt_decEqList(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_decEqList___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_decEq___boxed(lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_instDecidableEqSrt(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instDecidableEqSrt___boxed(lean_object*, lean_object*);
static lean_once_cell_t lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_default___lam__0___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_defaultList(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_default(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_default___lam__0(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Val_as(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Val_as___boxed(lean_object*, lean_object*);
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_Val_asList___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 1}, .m_objs = {((lean_object*)(((size_t)(0) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_Val_asList___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_Val_asList___closed__0_value;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Val_asList(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Val_asList___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_unpack(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_field(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_field___boxed(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Val_bv_x3f(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_boolOf(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_boolOf___boxed(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ctorIdx(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ctorIdx___boxed(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ctorElim(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ctorElim___boxed(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_not_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_not_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_and_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_and_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_or_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_or_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_xor_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_xor_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_implies_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_implies_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_eq_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_eq_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_distinct_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_distinct_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ite_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ite_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_add_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_add_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_sub_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_sub_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_mul_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_mul_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_div_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_div_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_mod_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_mod_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_abs_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_abs_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_lt_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_lt_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_le_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_le_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_gt_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_gt_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ge_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ge_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_select_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_select_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_store_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_store_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bv2nat_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bv2nat_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_int2bv_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_int2bv_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvadd_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvadd_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsub_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsub_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvmul_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvmul_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvudiv_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvudiv_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvurem_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvurem_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsdiv_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsdiv_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsrem_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsrem_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsmod_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsmod_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvneg_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvneg_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvand_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvand_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvor_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvor_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvxor_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvxor_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvnot_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvnot_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvshl_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvshl_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvlshr_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvlshr_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvashr_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvashr_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvult_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvult_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvule_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvule_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvugt_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvugt_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvuge_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvuge_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvslt_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvslt_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsle_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsle_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsgt_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsgt_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsge_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsge_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_concat_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_concat_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_extract_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_extract_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_zeroExtend_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_zeroExtend_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_signExtend_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_signExtend_elim(lean_object*, lean_object*, lean_object*, lean_object*);
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.concat"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__0_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__1_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__0_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__1 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__1_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__2_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvsge"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__2 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__2_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__3_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__2_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__3 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__3_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__4_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvsgt"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__4 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__4_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__5_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__4_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__5 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__5_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__6_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvsle"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__6 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__6_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__7_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__6_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__7 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__7_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__8_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvslt"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__8 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__8_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__9_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__8_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__9 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__9_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__10_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvuge"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__10 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__10_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__11_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__10_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__11 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__11_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__12_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvugt"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__12 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__12_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__13_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__12_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__13 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__13_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__14_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvule"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__14 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__14_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__15_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__14_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__15 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__15_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__16_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvult"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__16 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__16_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__17_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__16_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__17 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__17_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__18_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.bvashr"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__18 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__18_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__19_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__18_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__19 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__19_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__20_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.bvlshr"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__20 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__20_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__21_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__20_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__21 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__21_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__22_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvshl"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__22 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__22_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__23_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__22_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__23 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__23_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__24_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvnot"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__24 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__24_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__25_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__24_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__25 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__25_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__26_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvxor"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__26 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__26_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__27_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__26_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__27 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__27_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__28_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 24, .m_capacity = 24, .m_length = 23, .m_data = "SolarVerify.Smt.Op.bvor"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__28 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__28_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__29_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__28_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__29 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__29_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__30_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvand"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__30 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__30_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__31_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__30_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__31 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__31_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__32_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvneg"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__32 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__32_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__33_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__32_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__33 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__33_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__34_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.bvsmod"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__34 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__34_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__35_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__34_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__35 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__35_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__36_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.bvsrem"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__36 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__36_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__37_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__36_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__37 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__37_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__38_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.bvsdiv"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__38 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__38_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__39_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__38_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__39 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__39_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__40_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.bvurem"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__40 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__40_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__41_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__40_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__41 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__41_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__42_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.bvudiv"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__42 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__42_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__43_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__42_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__43 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__43_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__44_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvmul"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__44 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__44_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__45_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__44_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__45 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__45_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__46_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvsub"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__46 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__46_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__47_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__46_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__47 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__47_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__48_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.bvadd"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__48 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__48_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__49_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__48_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__49 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__49_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__50_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Op.not"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__50 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__50_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__51_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__50_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__51 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__51_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__52_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Op.and"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__52 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__52_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__53_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__52_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__53 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__53_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__54_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 22, .m_capacity = 22, .m_length = 21, .m_data = "SolarVerify.Smt.Op.or"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__54 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__54_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__55_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__54_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__55 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__55_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__56_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Op.xor"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__56 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__56_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__57_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__56_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__57 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__57_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__58_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 27, .m_capacity = 27, .m_length = 26, .m_data = "SolarVerify.Smt.Op.implies"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__58 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__58_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__59_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__58_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__59 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__59_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__60_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 22, .m_capacity = 22, .m_length = 21, .m_data = "SolarVerify.Smt.Op.eq"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__60 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__60_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__61_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__60_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__61 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__61_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__62_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 28, .m_capacity = 28, .m_length = 27, .m_data = "SolarVerify.Smt.Op.distinct"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__62 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__62_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__63_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__62_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__63 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__63_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__64_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Op.ite"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__64 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__64_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__65_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__64_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__65 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__65_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__66_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Op.add"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__66 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__66_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__67_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__66_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__67 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__67_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__68_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Op.sub"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__68 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__68_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__69_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__68_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__69 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__69_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__70_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Op.mul"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__70 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__70_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__71_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__70_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__71 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__71_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__72_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Op.div"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__72 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__72_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__73_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__72_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__73 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__73_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__74_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Op.mod"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__74 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__74_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__75_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__74_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__75 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__75_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__76_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 23, .m_capacity = 23, .m_length = 22, .m_data = "SolarVerify.Smt.Op.abs"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__76 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__76_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__77_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__76_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__77 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__77_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__78_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 22, .m_capacity = 22, .m_length = 21, .m_data = "SolarVerify.Smt.Op.lt"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__78 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__78_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__79_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__78_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__79 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__79_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__80_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 22, .m_capacity = 22, .m_length = 21, .m_data = "SolarVerify.Smt.Op.le"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__80 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__80_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__81_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__80_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__81 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__81_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__82_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 22, .m_capacity = 22, .m_length = 21, .m_data = "SolarVerify.Smt.Op.gt"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__82 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__82_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__83_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__82_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__83 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__83_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__84_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 22, .m_capacity = 22, .m_length = 21, .m_data = "SolarVerify.Smt.Op.ge"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__84 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__84_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__85_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__84_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__85 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__85_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__86_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.select"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__86 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__86_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__87_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__86_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__87 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__87_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__88_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Op.store"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__88 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__88_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__89_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__88_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__89 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__89_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__90_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.bv2nat"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__90 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__90_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__91_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__90_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__91 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__91_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__92_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Op.int2bv"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__92 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__92_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__93_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__92_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__93 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__93_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__94_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__93_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__94 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__94_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__95_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 27, .m_capacity = 27, .m_length = 26, .m_data = "SolarVerify.Smt.Op.extract"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__95 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__95_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__96_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__95_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__96 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__96_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__97_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__96_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__97 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__97_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__98_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 30, .m_capacity = 30, .m_length = 29, .m_data = "SolarVerify.Smt.Op.zeroExtend"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__98 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__98_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__99_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__98_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__99 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__99_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__100_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__99_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__100 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__100_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__101_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 30, .m_capacity = 30, .m_length = 29, .m_data = "SolarVerify.Smt.Op.signExtend"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__101 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__101_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__102_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__101_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__102 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__102_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__103_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__102_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__103 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__103_value;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___boxed(lean_object*, lean_object*);
static const lean_closure_object lp_solar__verify_SolarVerify_Smt_instReprOp___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_closure_object) + sizeof(void*)*0, .m_other = 0, .m_tag = 245}, .m_fun = (void*)lp_solar__verify_SolarVerify_Smt_instReprOp_repr___boxed, .m_arity = 2, .m_num_fixed = 0, .m_objs = {} };
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp___closed__0_value;
LEAN_EXPORT const lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprOp___closed__0_value;
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_instDecidableEqOp_decEq(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instDecidableEqOp_decEq___boxed(lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_instDecidableEqOp(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instDecidableEqOp___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedOp_default;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedOp;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_ctorIdx(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_ctorIdx___boxed(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_ctorElim(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_ctorElim___boxed(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_bool_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_bool_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_int_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_int_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_bv_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_bv_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_fn_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_fn_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_var_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_var_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_app_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_app_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_constArray_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_constArray_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_mk_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_mk_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_proj_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_proj_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_forall_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_forall_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_exists_elim___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_exists_elim(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1___redArg(lean_object*);
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Term.bool"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__0_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__1_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__0_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__1 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__1_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__2_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__1_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__2 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__2_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__3_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Term.int"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__3 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__3_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__4_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__3_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__4 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__4_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__5_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__4_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__5 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__5_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__6_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 24, .m_capacity = 24, .m_length = 23, .m_data = "SolarVerify.Smt.Term.bv"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__6 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__6_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__7_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__6_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__7 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__7_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__8_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__7_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__8 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__8_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__9_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 24, .m_capacity = 24, .m_length = 23, .m_data = "SolarVerify.Smt.Term.fn"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__9 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__9_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__10_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__9_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__10 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__10_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__11_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__10_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__11 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__11_value;
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0_spec__1_spec__3(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0_spec__1(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0___redArg(lean_object*);
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__12_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Term.var"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__12 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__12_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__13_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__12_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__13 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__13_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__14_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__13_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__14 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__14_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__15_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 25, .m_capacity = 25, .m_length = 24, .m_data = "SolarVerify.Smt.Term.app"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__15 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__15_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__16_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__15_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__16 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__16_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__17_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__16_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__17 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__17_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__18_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 32, .m_capacity = 32, .m_length = 31, .m_data = "SolarVerify.Smt.Term.constArray"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__18 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__18_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__19_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__18_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__19 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__19_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__20_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__19_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__20 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__20_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__21_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 24, .m_capacity = 24, .m_length = 23, .m_data = "SolarVerify.Smt.Term.mk"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__21 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__21_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__22_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__21_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__22 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__22_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__23_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__22_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__23 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__23_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__24_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 26, .m_capacity = 26, .m_length = 25, .m_data = "SolarVerify.Smt.Term.proj"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__24 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__24_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__25_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__24_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__25 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__25_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__26_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__25_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__26 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__26_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__27_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 28, .m_capacity = 28, .m_length = 27, .m_data = "SolarVerify.Smt.Term.forall"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__27 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__27_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__28_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__27_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__28 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__28_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__29_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__28_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__29 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__29_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__30_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 28, .m_capacity = 28, .m_length = 27, .m_data = "SolarVerify.Smt.Term.exists"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__30 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__30_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__31_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__30_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__31 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__31_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__32_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__31_value),((lean_object*)(((size_t)(1) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__32 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__32_value;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0___lam__0(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1___boxed(lean_object*, lean_object*);
static const lean_closure_object lp_solar__verify_SolarVerify_Smt_instReprTerm___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_closure_object) + sizeof(void*)*0, .m_other = 0, .m_tag = 245}, .m_fun = (void*)lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___boxed, .m_arity = 2, .m_num_fixed = 0, .m_objs = {} };
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm___closed__0_value;
LEAN_EXPORT const lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprTerm___closed__0_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instInhabitedTerm_default___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*0 + 8, .m_other = 0, .m_tag = 0}, .m_objs = {LEAN_SCALAR_PTR_LITERAL(0, 0, 0, 0, 0, 0, 0, 0)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedTerm_default___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instInhabitedTerm_default___closed__0_value;
LEAN_EXPORT const lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedTerm_default = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instInhabitedTerm_default___closed__0_value;
LEAN_EXPORT const lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedTerm = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instInhabitedTerm_default___closed__0_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 3, .m_capacity = 3, .m_length = 2, .m_data = "{ "};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__0_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__1_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 5, .m_capacity = 5, .m_length = 4, .m_data = "name"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__1 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__1_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__2_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__1_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__2 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__2_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__3_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)(((size_t)(0) << 1) | 1)),((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__2_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__3 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__3_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__4_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 5, .m_capacity = 5, .m_length = 4, .m_data = " := "};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__4 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__4_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__5_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__4_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__5 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__5_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__6_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*2 + 0, .m_other = 2, .m_tag = 5}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__3_value),((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__5_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__6 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__6_value;
static lean_once_cell_t lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__7_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__7;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__8_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 5, .m_capacity = 5, .m_length = 4, .m_data = "args"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__8 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__8_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__9_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__8_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__9 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__9_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__10_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 4, .m_capacity = 4, .m_length = 3, .m_data = "ret"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__10 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__10_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__11_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__10_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__11 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__11_value;
static lean_once_cell_t lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__12_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__12;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__13_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 3, .m_capacity = 3, .m_length = 2, .m_data = " }"};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__13 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__13_value;
static lean_once_cell_t lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__14_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__14;
static lean_once_cell_t lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__15_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__15;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__16_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__0_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__16 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__16_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__17_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 3}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__13_value)}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__17 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__17_value;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___boxed(lean_object*, lean_object*);
static const lean_closure_object lp_solar__verify_SolarVerify_Smt_instReprDecl___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_closure_object) + sizeof(void*)*0, .m_other = 0, .m_tag = 245}, .m_fun = (void*)lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___boxed, .m_arity = 2, .m_num_fixed = 0, .m_objs = {} };
static const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl___closed__0_value;
LEAN_EXPORT const lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instReprDecl___closed__0_value;
static const lean_string_object lp_solar__verify_SolarVerify_Smt_instInhabitedDecl_default___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = 0, .m_other = 0, .m_tag = 249}, .m_size = 1, .m_capacity = 1, .m_length = 0, .m_data = ""};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedDecl_default___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instInhabitedDecl_default___closed__0_value;
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_instInhabitedDecl_default___closed__1_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*3 + 0, .m_other = 3, .m_tag = 0}, .m_objs = {((lean_object*)&lp_solar__verify_SolarVerify_Smt_instInhabitedDecl_default___closed__0_value),((lean_object*)(((size_t)(0) << 1) | 1)),((lean_object*)(((size_t)(0) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedDecl_default___closed__1 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instInhabitedDecl_default___closed__1_value;
LEAN_EXPORT const lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedDecl_default = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instInhabitedDecl_default___closed__1_value;
LEAN_EXPORT const lean_object* lp_solar__verify_SolarVerify_Smt_instInhabitedDecl = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_instInhabitedDecl_default___closed__1_value;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_intDiv(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_intDiv___boxed(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_intMod(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_intMod___boxed(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_chain___redArg(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_chain___redArg___boxed(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_chain(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_chain___boxed(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0___redArg___boxed(lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_pairwise___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_pairwise___redArg___boxed(lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_pairwise(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_pairwise___boxed(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0___boxed(lean_object*, lean_object*, lean_object*);
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_ints___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 1}, .m_objs = {((lean_object*)(((size_t)(0) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_ints___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_ints___closed__0_value;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_ints(lean_object*);
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_bools___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 1}, .m_objs = {((lean_object*)(((size_t)(0) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_bools___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_bools___closed__0_value;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bools(lean_object*);
static const lean_ctor_object lp_solar__verify_SolarVerify_Smt_sameSort___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_ctor_object) + sizeof(void*)*1 + 0, .m_other = 1, .m_tag = 1}, .m_objs = {((lean_object*)(((size_t)(0) << 1) | 1))}};
static const lean_object* lp_solar__verify_SolarVerify_Smt_sameSort___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Smt_sameSort___closed__0_value;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_sameSort(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_sameSort___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bvs(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00SolarVerify_Smt_foldl2_spec__0___redArg(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_foldl2___redArg(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_foldl2(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00SolarVerify_Smt_foldl2_spec__0(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_List_foldl___at___00SolarVerify_Smt_impliesChain_spec__0(uint8_t, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00SolarVerify_Smt_impliesChain_spec__0___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_impliesChain(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_intRel(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bvRel(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bvFold(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bvBin(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bvUn(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_sortsOf(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_pack(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bindAll(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_ctorIdx(lean_object* v_x_1_){
_start:
{
switch(lean_obj_tag(v_x_1_))
{
case 0:
{
lean_object* v___x_2_; 
v___x_2_ = lean_unsigned_to_nat(0u);
return v___x_2_;
}
case 1:
{
lean_object* v___x_3_; 
v___x_3_ = lean_unsigned_to_nat(1u);
return v___x_3_;
}
case 2:
{
lean_object* v___x_4_; 
v___x_4_ = lean_unsigned_to_nat(2u);
return v___x_4_;
}
case 3:
{
lean_object* v___x_5_; 
v___x_5_ = lean_unsigned_to_nat(3u);
return v___x_5_;
}
default: 
{
lean_object* v___x_6_; 
v___x_6_ = lean_unsigned_to_nat(4u);
return v___x_6_;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_ctorIdx___boxed(lean_object* v_x_7_){
_start:
{
lean_object* v_res_8_; 
v_res_8_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorIdx(v_x_7_);
lean_dec(v_x_7_);
return v_res_8_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(lean_object* v_t_9_, lean_object* v_k_10_){
_start:
{
switch(lean_obj_tag(v_t_9_))
{
case 2:
{
lean_object* v_w_11_; lean_object* v___x_12_; 
v_w_11_ = lean_ctor_get(v_t_9_, 0);
lean_inc(v_w_11_);
lean_dec_ref_known(v_t_9_, 1);
v___x_12_ = lean_apply_1(v_k_10_, v_w_11_);
return v___x_12_;
}
case 3:
{
lean_object* v_idx_13_; lean_object* v_elem_14_; lean_object* v___x_15_; 
v_idx_13_ = lean_ctor_get(v_t_9_, 0);
lean_inc(v_idx_13_);
v_elem_14_ = lean_ctor_get(v_t_9_, 1);
lean_inc(v_elem_14_);
lean_dec_ref_known(v_t_9_, 2);
v___x_15_ = lean_apply_2(v_k_10_, v_idx_13_, v_elem_14_);
return v___x_15_;
}
case 4:
{
lean_object* v_fields_16_; lean_object* v___x_17_; 
v_fields_16_ = lean_ctor_get(v_t_9_, 0);
lean_inc(v_fields_16_);
lean_dec_ref_known(v_t_9_, 1);
v___x_17_ = lean_apply_1(v_k_10_, v_fields_16_);
return v___x_17_;
}
default: 
{
lean_dec(v_t_9_);
return v_k_10_;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_ctorElim(lean_object* v_motive__1_18_, lean_object* v_ctorIdx_19_, lean_object* v_t_20_, lean_object* v_h_21_, lean_object* v_k_22_){
_start:
{
lean_object* v___x_23_; 
v___x_23_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_20_, v_k_22_);
return v___x_23_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___boxed(lean_object* v_motive__1_24_, lean_object* v_ctorIdx_25_, lean_object* v_t_26_, lean_object* v_h_27_, lean_object* v_k_28_){
_start:
{
lean_object* v_res_29_; 
v_res_29_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim(v_motive__1_24_, v_ctorIdx_25_, v_t_26_, v_h_27_, v_k_28_);
lean_dec(v_ctorIdx_25_);
return v_res_29_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_bool_elim___redArg(lean_object* v_t_30_, lean_object* v_bool_31_){
_start:
{
lean_object* v___x_32_; 
v___x_32_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_30_, v_bool_31_);
return v___x_32_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_bool_elim(lean_object* v_motive__1_33_, lean_object* v_t_34_, lean_object* v_h_35_, lean_object* v_bool_36_){
_start:
{
lean_object* v___x_37_; 
v___x_37_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_34_, v_bool_36_);
return v___x_37_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_int_elim___redArg(lean_object* v_t_38_, lean_object* v_int_39_){
_start:
{
lean_object* v___x_40_; 
v___x_40_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_38_, v_int_39_);
return v___x_40_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_int_elim(lean_object* v_motive__1_41_, lean_object* v_t_42_, lean_object* v_h_43_, lean_object* v_int_44_){
_start:
{
lean_object* v___x_45_; 
v___x_45_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_42_, v_int_44_);
return v___x_45_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_bv_elim___redArg(lean_object* v_t_46_, lean_object* v_bv_47_){
_start:
{
lean_object* v___x_48_; 
v___x_48_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_46_, v_bv_47_);
return v___x_48_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_bv_elim(lean_object* v_motive__1_49_, lean_object* v_t_50_, lean_object* v_h_51_, lean_object* v_bv_52_){
_start:
{
lean_object* v___x_53_; 
v___x_53_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_50_, v_bv_52_);
return v___x_53_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_array_elim___redArg(lean_object* v_t_54_, lean_object* v_array_55_){
_start:
{
lean_object* v___x_56_; 
v___x_56_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_54_, v_array_55_);
return v___x_56_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_array_elim(lean_object* v_motive__1_57_, lean_object* v_t_58_, lean_object* v_h_59_, lean_object* v_array_60_){
_start:
{
lean_object* v___x_61_; 
v___x_61_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_58_, v_array_60_);
return v___x_61_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_tup_elim___redArg(lean_object* v_t_62_, lean_object* v_tup_63_){
_start:
{
lean_object* v___x_64_; 
v___x_64_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_62_, v_tup_63_);
return v___x_64_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_tup_elim(lean_object* v_motive__1_65_, lean_object* v_t_66_, lean_object* v_h_67_, lean_object* v_tup_68_){
_start:
{
lean_object* v___x_69_; 
v___x_69_ = lp_solar__verify_SolarVerify_Smt_Srt_ctorElim___redArg(v_t_66_, v_tup_68_);
return v___x_69_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4(void){
_start:
{
lean_object* v___x_76_; lean_object* v___x_77_; 
v___x_76_ = lean_unsigned_to_nat(2u);
v___x_77_ = lean_nat_to_int(v___x_76_);
return v___x_77_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5(void){
_start:
{
lean_object* v___x_78_; lean_object* v___x_79_; 
v___x_78_ = lean_unsigned_to_nat(1u);
v___x_79_ = lean_nat_to_int(v___x_78_);
return v___x_79_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0_spec__1_spec__2(lean_object* v_x_107_, lean_object* v_x_108_, lean_object* v_x_109_){
_start:
{
if (lean_obj_tag(v_x_109_) == 0)
{
lean_dec(v_x_107_);
return v_x_108_;
}
else
{
lean_object* v_head_110_; lean_object* v_tail_111_; lean_object* v___x_113_; uint8_t v_isShared_114_; uint8_t v_isSharedCheck_122_; 
v_head_110_ = lean_ctor_get(v_x_109_, 0);
v_tail_111_ = lean_ctor_get(v_x_109_, 1);
v_isSharedCheck_122_ = !lean_is_exclusive(v_x_109_);
if (v_isSharedCheck_122_ == 0)
{
v___x_113_ = v_x_109_;
v_isShared_114_ = v_isSharedCheck_122_;
goto v_resetjp_112_;
}
else
{
lean_inc(v_tail_111_);
lean_inc(v_head_110_);
lean_dec(v_x_109_);
v___x_113_ = lean_box(0);
v_isShared_114_ = v_isSharedCheck_122_;
goto v_resetjp_112_;
}
v_resetjp_112_:
{
lean_object* v___x_116_; 
lean_inc(v_x_107_);
if (v_isShared_114_ == 0)
{
lean_ctor_set_tag(v___x_113_, 5);
lean_ctor_set(v___x_113_, 1, v_x_107_);
lean_ctor_set(v___x_113_, 0, v_x_108_);
v___x_116_ = v___x_113_;
goto v_reusejp_115_;
}
else
{
lean_object* v_reuseFailAlloc_121_; 
v_reuseFailAlloc_121_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_121_, 0, v_x_108_);
lean_ctor_set(v_reuseFailAlloc_121_, 1, v_x_107_);
v___x_116_ = v_reuseFailAlloc_121_;
goto v_reusejp_115_;
}
v_reusejp_115_:
{
lean_object* v___x_117_; lean_object* v___x_118_; lean_object* v___x_119_; 
v___x_117_ = lean_unsigned_to_nat(0u);
v___x_118_ = lp_solar__verify_SolarVerify_Smt_instReprSrt_repr(v_head_110_, v___x_117_);
v___x_119_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_119_, 0, v___x_116_);
lean_ctor_set(v___x_119_, 1, v___x_118_);
v_x_108_ = v___x_119_;
v_x_109_ = v_tail_111_;
goto _start;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0_spec__1(lean_object* v_x_123_, lean_object* v_x_124_, lean_object* v_x_125_){
_start:
{
if (lean_obj_tag(v_x_125_) == 0)
{
lean_dec(v_x_123_);
return v_x_124_;
}
else
{
lean_object* v_head_126_; lean_object* v_tail_127_; lean_object* v___x_129_; uint8_t v_isShared_130_; uint8_t v_isSharedCheck_138_; 
v_head_126_ = lean_ctor_get(v_x_125_, 0);
v_tail_127_ = lean_ctor_get(v_x_125_, 1);
v_isSharedCheck_138_ = !lean_is_exclusive(v_x_125_);
if (v_isSharedCheck_138_ == 0)
{
v___x_129_ = v_x_125_;
v_isShared_130_ = v_isSharedCheck_138_;
goto v_resetjp_128_;
}
else
{
lean_inc(v_tail_127_);
lean_inc(v_head_126_);
lean_dec(v_x_125_);
v___x_129_ = lean_box(0);
v_isShared_130_ = v_isSharedCheck_138_;
goto v_resetjp_128_;
}
v_resetjp_128_:
{
lean_object* v___x_132_; 
lean_inc(v_x_123_);
if (v_isShared_130_ == 0)
{
lean_ctor_set_tag(v___x_129_, 5);
lean_ctor_set(v___x_129_, 1, v_x_123_);
lean_ctor_set(v___x_129_, 0, v_x_124_);
v___x_132_ = v___x_129_;
goto v_reusejp_131_;
}
else
{
lean_object* v_reuseFailAlloc_137_; 
v_reuseFailAlloc_137_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_137_, 0, v_x_124_);
lean_ctor_set(v_reuseFailAlloc_137_, 1, v_x_123_);
v___x_132_ = v_reuseFailAlloc_137_;
goto v_reusejp_131_;
}
v_reusejp_131_:
{
lean_object* v___x_133_; lean_object* v___x_134_; lean_object* v___x_135_; lean_object* v___x_136_; 
v___x_133_ = lean_unsigned_to_nat(0u);
v___x_134_ = lp_solar__verify_SolarVerify_Smt_instReprSrt_repr(v_head_126_, v___x_133_);
v___x_135_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_135_, 0, v___x_132_);
lean_ctor_set(v___x_135_, 1, v___x_134_);
v___x_136_ = lp_solar__verify_List_foldl___at___00List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0_spec__1_spec__2(v_x_123_, v___x_135_, v_tail_127_);
return v___x_136_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0(lean_object* v_x_139_, lean_object* v_x_140_){
_start:
{
if (lean_obj_tag(v_x_139_) == 0)
{
lean_object* v___x_141_; 
lean_dec(v_x_140_);
v___x_141_ = lean_box(0);
return v___x_141_;
}
else
{
lean_object* v_tail_142_; 
v_tail_142_ = lean_ctor_get(v_x_139_, 1);
if (lean_obj_tag(v_tail_142_) == 0)
{
lean_object* v_head_143_; lean_object* v___x_144_; 
lean_dec(v_x_140_);
v_head_143_ = lean_ctor_get(v_x_139_, 0);
lean_inc(v_head_143_);
lean_dec_ref_known(v_x_139_, 2);
v___x_144_ = lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0___lam__0(v_head_143_);
return v___x_144_;
}
else
{
lean_object* v_head_145_; lean_object* v___x_146_; lean_object* v___x_147_; 
lean_inc(v_tail_142_);
v_head_145_ = lean_ctor_get(v_x_139_, 0);
lean_inc(v_head_145_);
lean_dec_ref_known(v_x_139_, 2);
v___x_146_ = lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0___lam__0(v_head_145_);
v___x_147_ = lp_solar__verify_List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0_spec__1(v_x_140_, v___x_146_, v_tail_142_);
return v___x_147_;
}
}
}
}
static lean_object* _init_lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__7(void){
_start:
{
lean_object* v___x_149_; lean_object* v___x_150_; 
v___x_149_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__2));
v___x_150_ = lean_string_length(v___x_149_);
return v___x_150_;
}
}
static lean_object* _init_lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8(void){
_start:
{
lean_object* v___x_151_; lean_object* v___x_152_; 
v___x_151_ = lean_obj_once(&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__7, &lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__7_once, _init_lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__7);
v___x_152_ = lean_nat_to_int(v___x_151_);
return v___x_152_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg(lean_object* v_a_158_){
_start:
{
if (lean_obj_tag(v_a_158_) == 0)
{
lean_object* v___x_159_; 
v___x_159_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__1));
return v___x_159_;
}
else
{
lean_object* v___x_160_; lean_object* v___x_161_; lean_object* v___x_162_; lean_object* v___x_163_; lean_object* v___x_164_; lean_object* v___x_165_; lean_object* v___x_166_; lean_object* v___x_167_; uint8_t v___x_168_; lean_object* v___x_169_; 
v___x_160_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__5));
v___x_161_ = lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0(v_a_158_, v___x_160_);
v___x_162_ = lean_obj_once(&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8, &lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8_once, _init_lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8);
v___x_163_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__9));
v___x_164_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_164_, 0, v___x_163_);
lean_ctor_set(v___x_164_, 1, v___x_161_);
v___x_165_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__10));
v___x_166_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_166_, 0, v___x_164_);
lean_ctor_set(v___x_166_, 1, v___x_165_);
v___x_167_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_167_, 0, v___x_162_);
lean_ctor_set(v___x_167_, 1, v___x_166_);
v___x_168_ = 0;
v___x_169_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_169_, 0, v___x_167_);
lean_ctor_set_uint8(v___x_169_, sizeof(void*)*1, v___x_168_);
return v___x_169_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr(lean_object* v_x_170_, lean_object* v_prec_171_){
_start:
{
lean_object* v___y_173_; lean_object* v___y_180_; 
switch(lean_obj_tag(v_x_170_))
{
case 0:
{
lean_object* v___x_186_; uint8_t v___x_187_; 
v___x_186_ = lean_unsigned_to_nat(1024u);
v___x_187_ = lean_nat_dec_le(v___x_186_, v_prec_171_);
if (v___x_187_ == 0)
{
lean_object* v___x_188_; 
v___x_188_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_180_ = v___x_188_;
goto v___jp_179_;
}
else
{
lean_object* v___x_189_; 
v___x_189_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_180_ = v___x_189_;
goto v___jp_179_;
}
}
case 1:
{
lean_object* v___x_190_; uint8_t v___x_191_; 
v___x_190_ = lean_unsigned_to_nat(1024u);
v___x_191_ = lean_nat_dec_le(v___x_190_, v_prec_171_);
if (v___x_191_ == 0)
{
lean_object* v___x_192_; 
v___x_192_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_173_ = v___x_192_;
goto v___jp_172_;
}
else
{
lean_object* v___x_193_; 
v___x_193_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_173_ = v___x_193_;
goto v___jp_172_;
}
}
case 2:
{
lean_object* v_w_194_; lean_object* v___x_196_; uint8_t v_isShared_197_; uint8_t v_isSharedCheck_214_; 
v_w_194_ = lean_ctor_get(v_x_170_, 0);
v_isSharedCheck_214_ = !lean_is_exclusive(v_x_170_);
if (v_isSharedCheck_214_ == 0)
{
v___x_196_ = v_x_170_;
v_isShared_197_ = v_isSharedCheck_214_;
goto v_resetjp_195_;
}
else
{
lean_inc(v_w_194_);
lean_dec(v_x_170_);
v___x_196_ = lean_box(0);
v_isShared_197_ = v_isSharedCheck_214_;
goto v_resetjp_195_;
}
v_resetjp_195_:
{
lean_object* v___y_199_; lean_object* v___x_210_; uint8_t v___x_211_; 
v___x_210_ = lean_unsigned_to_nat(1024u);
v___x_211_ = lean_nat_dec_le(v___x_210_, v_prec_171_);
if (v___x_211_ == 0)
{
lean_object* v___x_212_; 
v___x_212_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_199_ = v___x_212_;
goto v___jp_198_;
}
else
{
lean_object* v___x_213_; 
v___x_213_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_199_ = v___x_213_;
goto v___jp_198_;
}
v___jp_198_:
{
lean_object* v___x_200_; lean_object* v___x_201_; lean_object* v___x_203_; 
v___x_200_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__8));
v___x_201_ = l_Nat_reprFast(v_w_194_);
if (v_isShared_197_ == 0)
{
lean_ctor_set_tag(v___x_196_, 3);
lean_ctor_set(v___x_196_, 0, v___x_201_);
v___x_203_ = v___x_196_;
goto v_reusejp_202_;
}
else
{
lean_object* v_reuseFailAlloc_209_; 
v_reuseFailAlloc_209_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v_reuseFailAlloc_209_, 0, v___x_201_);
v___x_203_ = v_reuseFailAlloc_209_;
goto v_reusejp_202_;
}
v_reusejp_202_:
{
lean_object* v___x_204_; lean_object* v___x_205_; uint8_t v___x_206_; lean_object* v___x_207_; lean_object* v___x_208_; 
v___x_204_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_204_, 0, v___x_200_);
lean_ctor_set(v___x_204_, 1, v___x_203_);
lean_inc(v___y_199_);
v___x_205_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_205_, 0, v___y_199_);
lean_ctor_set(v___x_205_, 1, v___x_204_);
v___x_206_ = 0;
v___x_207_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_207_, 0, v___x_205_);
lean_ctor_set_uint8(v___x_207_, sizeof(void*)*1, v___x_206_);
v___x_208_ = l_Repr_addAppParen(v___x_207_, v_prec_171_);
return v___x_208_;
}
}
}
}
case 3:
{
lean_object* v_idx_215_; lean_object* v_elem_216_; lean_object* v___x_218_; uint8_t v_isShared_219_; uint8_t v_isSharedCheck_239_; 
v_idx_215_ = lean_ctor_get(v_x_170_, 0);
v_elem_216_ = lean_ctor_get(v_x_170_, 1);
v_isSharedCheck_239_ = !lean_is_exclusive(v_x_170_);
if (v_isSharedCheck_239_ == 0)
{
v___x_218_ = v_x_170_;
v_isShared_219_ = v_isSharedCheck_239_;
goto v_resetjp_217_;
}
else
{
lean_inc(v_elem_216_);
lean_inc(v_idx_215_);
lean_dec(v_x_170_);
v___x_218_ = lean_box(0);
v_isShared_219_ = v_isSharedCheck_239_;
goto v_resetjp_217_;
}
v_resetjp_217_:
{
lean_object* v___x_220_; lean_object* v___y_222_; uint8_t v___x_236_; 
v___x_220_ = lean_unsigned_to_nat(1024u);
v___x_236_ = lean_nat_dec_le(v___x_220_, v_prec_171_);
if (v___x_236_ == 0)
{
lean_object* v___x_237_; 
v___x_237_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_222_ = v___x_237_;
goto v___jp_221_;
}
else
{
lean_object* v___x_238_; 
v___x_238_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_222_ = v___x_238_;
goto v___jp_221_;
}
v___jp_221_:
{
lean_object* v___x_223_; lean_object* v___x_224_; lean_object* v___x_225_; lean_object* v___x_227_; 
v___x_223_ = lean_box(1);
v___x_224_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__11));
v___x_225_ = lp_solar__verify_SolarVerify_Smt_instReprSrt_repr(v_idx_215_, v___x_220_);
if (v_isShared_219_ == 0)
{
lean_ctor_set_tag(v___x_218_, 5);
lean_ctor_set(v___x_218_, 1, v___x_225_);
lean_ctor_set(v___x_218_, 0, v___x_224_);
v___x_227_ = v___x_218_;
goto v_reusejp_226_;
}
else
{
lean_object* v_reuseFailAlloc_235_; 
v_reuseFailAlloc_235_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_235_, 0, v___x_224_);
lean_ctor_set(v_reuseFailAlloc_235_, 1, v___x_225_);
v___x_227_ = v_reuseFailAlloc_235_;
goto v_reusejp_226_;
}
v_reusejp_226_:
{
lean_object* v___x_228_; lean_object* v___x_229_; lean_object* v___x_230_; lean_object* v___x_231_; uint8_t v___x_232_; lean_object* v___x_233_; lean_object* v___x_234_; 
v___x_228_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_228_, 0, v___x_227_);
lean_ctor_set(v___x_228_, 1, v___x_223_);
v___x_229_ = lp_solar__verify_SolarVerify_Smt_instReprSrt_repr(v_elem_216_, v___x_220_);
v___x_230_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_230_, 0, v___x_228_);
lean_ctor_set(v___x_230_, 1, v___x_229_);
lean_inc(v___y_222_);
v___x_231_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_231_, 0, v___y_222_);
lean_ctor_set(v___x_231_, 1, v___x_230_);
v___x_232_ = 0;
v___x_233_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_233_, 0, v___x_231_);
lean_ctor_set_uint8(v___x_233_, sizeof(void*)*1, v___x_232_);
v___x_234_ = l_Repr_addAppParen(v___x_233_, v_prec_171_);
return v___x_234_;
}
}
}
}
default: 
{
lean_object* v_fields_240_; lean_object* v___y_242_; lean_object* v___x_250_; uint8_t v___x_251_; 
v_fields_240_ = lean_ctor_get(v_x_170_, 0);
lean_inc(v_fields_240_);
lean_dec_ref_known(v_x_170_, 1);
v___x_250_ = lean_unsigned_to_nat(1024u);
v___x_251_ = lean_nat_dec_le(v___x_250_, v_prec_171_);
if (v___x_251_ == 0)
{
lean_object* v___x_252_; 
v___x_252_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_242_ = v___x_252_;
goto v___jp_241_;
}
else
{
lean_object* v___x_253_; 
v___x_253_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_242_ = v___x_253_;
goto v___jp_241_;
}
v___jp_241_:
{
lean_object* v___x_243_; lean_object* v___x_244_; lean_object* v___x_245_; lean_object* v___x_246_; uint8_t v___x_247_; lean_object* v___x_248_; lean_object* v___x_249_; 
v___x_243_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__14));
v___x_244_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg(v_fields_240_);
v___x_245_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_245_, 0, v___x_243_);
lean_ctor_set(v___x_245_, 1, v___x_244_);
lean_inc(v___y_242_);
v___x_246_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_246_, 0, v___y_242_);
lean_ctor_set(v___x_246_, 1, v___x_245_);
v___x_247_ = 0;
v___x_248_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_248_, 0, v___x_246_);
lean_ctor_set_uint8(v___x_248_, sizeof(void*)*1, v___x_247_);
v___x_249_ = l_Repr_addAppParen(v___x_248_, v_prec_171_);
return v___x_249_;
}
}
}
v___jp_172_:
{
lean_object* v___x_174_; lean_object* v___x_175_; uint8_t v___x_176_; lean_object* v___x_177_; lean_object* v___x_178_; 
v___x_174_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__1));
lean_inc(v___y_173_);
v___x_175_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_175_, 0, v___y_173_);
lean_ctor_set(v___x_175_, 1, v___x_174_);
v___x_176_ = 0;
v___x_177_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_177_, 0, v___x_175_);
lean_ctor_set_uint8(v___x_177_, sizeof(void*)*1, v___x_176_);
v___x_178_ = l_Repr_addAppParen(v___x_177_, v_prec_171_);
return v___x_178_;
}
v___jp_179_:
{
lean_object* v___x_181_; lean_object* v___x_182_; uint8_t v___x_183_; lean_object* v___x_184_; lean_object* v___x_185_; 
v___x_181_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__3));
lean_inc(v___y_180_);
v___x_182_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_182_, 0, v___y_180_);
lean_ctor_set(v___x_182_, 1, v___x_181_);
v___x_183_ = 0;
v___x_184_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_184_, 0, v___x_182_);
lean_ctor_set_uint8(v___x_184_, sizeof(void*)*1, v___x_183_);
v___x_185_ = l_Repr_addAppParen(v___x_184_, v_prec_171_);
return v___x_185_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0___lam__0(lean_object* v___y_254_){
_start:
{
lean_object* v___x_255_; lean_object* v___x_256_; 
v___x_255_ = lean_unsigned_to_nat(0u);
v___x_256_ = lp_solar__verify_SolarVerify_Smt_instReprSrt_repr(v___y_254_, v___x_255_);
return v___x_256_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___boxed(lean_object* v_x_257_, lean_object* v_prec_258_){
_start:
{
lean_object* v_res_259_; 
v_res_259_ = lp_solar__verify_SolarVerify_Smt_instReprSrt_repr(v_x_257_, v_prec_258_);
lean_dec(v_prec_258_);
return v_res_259_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0(lean_object* v_a_260_, lean_object* v_n_261_){
_start:
{
lean_object* v___x_262_; 
v___x_262_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg(v_a_260_);
return v___x_262_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___boxed(lean_object* v_a_263_, lean_object* v_n_264_){
_start:
{
lean_object* v_res_265_; 
v_res_265_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0(v_a_263_, v_n_264_);
lean_dec(v_n_264_);
return v_res_265_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_instInhabitedSrt_default(void){
_start:
{
lean_object* v___x_268_; 
v___x_268_ = lean_box(0);
return v___x_268_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_instInhabitedSrt(void){
_start:
{
lean_object* v___x_269_; 
v___x_269_ = lean_box(0);
return v___x_269_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_Srt_decEq(lean_object* v_x_270_, lean_object* v_x_271_){
_start:
{
switch(lean_obj_tag(v_x_270_))
{
case 0:
{
if (lean_obj_tag(v_x_271_) == 0)
{
uint8_t v___x_272_; 
v___x_272_ = 1;
return v___x_272_;
}
else
{
uint8_t v___x_273_; 
v___x_273_ = 0;
return v___x_273_;
}
}
case 1:
{
if (lean_obj_tag(v_x_271_) == 1)
{
uint8_t v___x_274_; 
v___x_274_ = 1;
return v___x_274_;
}
else
{
uint8_t v___x_275_; 
v___x_275_ = 0;
return v___x_275_;
}
}
case 2:
{
if (lean_obj_tag(v_x_271_) == 2)
{
lean_object* v_w_276_; lean_object* v_w_277_; uint8_t v___x_278_; 
v_w_276_ = lean_ctor_get(v_x_270_, 0);
v_w_277_ = lean_ctor_get(v_x_271_, 0);
v___x_278_ = lean_nat_dec_eq(v_w_276_, v_w_277_);
return v___x_278_;
}
else
{
uint8_t v___x_279_; 
v___x_279_ = 0;
return v___x_279_;
}
}
case 3:
{
if (lean_obj_tag(v_x_271_) == 3)
{
lean_object* v_idx_280_; lean_object* v_elem_281_; lean_object* v_idx_282_; lean_object* v_elem_283_; uint8_t v_decide_284_; 
v_idx_280_ = lean_ctor_get(v_x_270_, 0);
v_elem_281_ = lean_ctor_get(v_x_270_, 1);
v_idx_282_ = lean_ctor_get(v_x_271_, 0);
v_elem_283_ = lean_ctor_get(v_x_271_, 1);
v_decide_284_ = lp_solar__verify_SolarVerify_Smt_Srt_decEq(v_idx_280_, v_idx_282_);
if (v_decide_284_ == 0)
{
return v_decide_284_;
}
else
{
v_x_270_ = v_elem_281_;
v_x_271_ = v_elem_283_;
goto _start;
}
}
else
{
uint8_t v___x_286_; 
v___x_286_ = 0;
return v___x_286_;
}
}
default: 
{
if (lean_obj_tag(v_x_271_) == 4)
{
lean_object* v_fields_287_; lean_object* v_fields_288_; uint8_t v_decide_289_; 
v_fields_287_ = lean_ctor_get(v_x_270_, 0);
v_fields_288_ = lean_ctor_get(v_x_271_, 0);
v_decide_289_ = lp_solar__verify_SolarVerify_Smt_Srt_decEqList(v_fields_287_, v_fields_288_);
return v_decide_289_;
}
else
{
uint8_t v___x_290_; 
v___x_290_ = 0;
return v___x_290_;
}
}
}
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_Srt_decEqList(lean_object* v_x_291_, lean_object* v_x_292_){
_start:
{
if (lean_obj_tag(v_x_291_) == 0)
{
if (lean_obj_tag(v_x_292_) == 0)
{
uint8_t v___x_293_; 
v___x_293_ = 1;
return v___x_293_;
}
else
{
uint8_t v___x_294_; 
v___x_294_ = 0;
return v___x_294_;
}
}
else
{
if (lean_obj_tag(v_x_292_) == 0)
{
uint8_t v___x_295_; 
v___x_295_ = 0;
return v___x_295_;
}
else
{
lean_object* v_head_296_; lean_object* v_tail_297_; lean_object* v_head_298_; lean_object* v_tail_299_; uint8_t v_decide_300_; 
v_head_296_ = lean_ctor_get(v_x_291_, 0);
v_tail_297_ = lean_ctor_get(v_x_291_, 1);
v_head_298_ = lean_ctor_get(v_x_292_, 0);
v_tail_299_ = lean_ctor_get(v_x_292_, 1);
v_decide_300_ = lp_solar__verify_SolarVerify_Smt_Srt_decEq(v_head_296_, v_head_298_);
if (v_decide_300_ == 0)
{
return v_decide_300_;
}
else
{
v_x_291_ = v_tail_297_;
v_x_292_ = v_tail_299_;
goto _start;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_decEqList___boxed(lean_object* v_x_302_, lean_object* v_x_303_){
_start:
{
uint8_t v_res_304_; lean_object* v_r_305_; 
v_res_304_ = lp_solar__verify_SolarVerify_Smt_Srt_decEqList(v_x_302_, v_x_303_);
lean_dec(v_x_303_);
lean_dec(v_x_302_);
v_r_305_ = lean_box(v_res_304_);
return v_r_305_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_decEq___boxed(lean_object* v_x_306_, lean_object* v_x_307_){
_start:
{
uint8_t v_res_308_; lean_object* v_r_309_; 
v_res_308_ = lp_solar__verify_SolarVerify_Smt_Srt_decEq(v_x_306_, v_x_307_);
lean_dec(v_x_307_);
lean_dec(v_x_306_);
v_r_309_ = lean_box(v_res_308_);
return v_r_309_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_instDecidableEqSrt(lean_object* v_a_310_, lean_object* v_b_311_){
_start:
{
uint8_t v___x_312_; 
v___x_312_ = lp_solar__verify_SolarVerify_Smt_Srt_decEq(v_a_310_, v_b_311_);
return v___x_312_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instDecidableEqSrt___boxed(lean_object* v_a_313_, lean_object* v_b_314_){
_start:
{
uint8_t v_res_315_; lean_object* v_r_316_; 
v_res_315_ = lp_solar__verify_SolarVerify_Smt_instDecidableEqSrt(v_a_313_, v_b_314_);
lean_dec(v_b_314_);
lean_dec(v_a_313_);
v_r_316_ = lean_box(v_res_315_);
return v_r_316_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0(void){
_start:
{
lean_object* v___x_317_; lean_object* v___x_318_; 
v___x_317_ = lean_unsigned_to_nat(0u);
v___x_318_ = lean_nat_to_int(v___x_317_);
return v___x_318_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_default___lam__0___boxed(lean_object* v_elem_319_, lean_object* v___y_320_){
_start:
{
lean_object* v_res_321_; 
v_res_321_ = lp_solar__verify_SolarVerify_Smt_Srt_default___lam__0(v_elem_319_, v___y_320_);
lean_dec(v___y_320_);
return v_res_321_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_defaultList(lean_object* v_x_322_){
_start:
{
if (lean_obj_tag(v_x_322_) == 0)
{
lean_object* v___x_323_; 
v___x_323_ = lean_box(0);
return v___x_323_;
}
else
{
lean_object* v_head_324_; lean_object* v_tail_325_; lean_object* v___x_327_; uint8_t v_isShared_328_; uint8_t v_isSharedCheck_334_; 
v_head_324_ = lean_ctor_get(v_x_322_, 0);
v_tail_325_ = lean_ctor_get(v_x_322_, 1);
v_isSharedCheck_334_ = !lean_is_exclusive(v_x_322_);
if (v_isSharedCheck_334_ == 0)
{
v___x_327_ = v_x_322_;
v_isShared_328_ = v_isSharedCheck_334_;
goto v_resetjp_326_;
}
else
{
lean_inc(v_tail_325_);
lean_inc(v_head_324_);
lean_dec(v_x_322_);
v___x_327_ = lean_box(0);
v_isShared_328_ = v_isSharedCheck_334_;
goto v_resetjp_326_;
}
v_resetjp_326_:
{
lean_object* v___x_329_; lean_object* v___x_330_; lean_object* v___x_332_; 
v___x_329_ = lp_solar__verify_SolarVerify_Smt_Srt_default(v_head_324_);
v___x_330_ = lp_solar__verify_SolarVerify_Smt_Srt_defaultList(v_tail_325_);
if (v_isShared_328_ == 0)
{
lean_ctor_set_tag(v___x_327_, 0);
lean_ctor_set(v___x_327_, 1, v___x_330_);
lean_ctor_set(v___x_327_, 0, v___x_329_);
v___x_332_ = v___x_327_;
goto v_reusejp_331_;
}
else
{
lean_object* v_reuseFailAlloc_333_; 
v_reuseFailAlloc_333_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_333_, 0, v___x_329_);
lean_ctor_set(v_reuseFailAlloc_333_, 1, v___x_330_);
v___x_332_ = v_reuseFailAlloc_333_;
goto v_reusejp_331_;
}
v_reusejp_331_:
{
return v___x_332_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_default(lean_object* v_x_335_){
_start:
{
switch(lean_obj_tag(v_x_335_))
{
case 0:
{
uint8_t v___x_336_; lean_object* v___x_337_; 
v___x_336_ = 0;
v___x_337_ = lean_box(v___x_336_);
return v___x_337_;
}
case 1:
{
lean_object* v___x_338_; 
v___x_338_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0, &lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0_once, _init_lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0);
return v___x_338_;
}
case 2:
{
lean_object* v_w_339_; lean_object* v___x_340_; lean_object* v___x_341_; 
v_w_339_ = lean_ctor_get(v_x_335_, 0);
lean_inc(v_w_339_);
lean_dec_ref_known(v_x_335_, 1);
v___x_340_ = lean_unsigned_to_nat(0u);
v___x_341_ = l_BitVec_ofNat(v_w_339_, v___x_340_);
lean_dec(v_w_339_);
return v___x_341_;
}
case 3:
{
lean_object* v_elem_342_; lean_object* v___f_343_; 
v_elem_342_ = lean_ctor_get(v_x_335_, 1);
lean_inc(v_elem_342_);
lean_dec_ref_known(v_x_335_, 2);
v___f_343_ = lean_alloc_closure((void*)(lp_solar__verify_SolarVerify_Smt_Srt_default___lam__0___boxed), 2, 1);
lean_closure_set(v___f_343_, 0, v_elem_342_);
return v___f_343_;
}
default: 
{
lean_object* v_fields_344_; lean_object* v___x_345_; 
v_fields_344_ = lean_ctor_get(v_x_335_, 0);
lean_inc(v_fields_344_);
lean_dec_ref_known(v_x_335_, 1);
v___x_345_ = lp_solar__verify_SolarVerify_Smt_Srt_defaultList(v_fields_344_);
return v___x_345_;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_default___lam__0(lean_object* v_elem_346_, lean_object* v___y_347_){
_start:
{
lean_object* v___x_348_; 
v___x_348_ = lp_solar__verify_SolarVerify_Smt_Srt_default(v_elem_346_);
return v___x_348_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Val_as(lean_object* v_v_349_, lean_object* v_s_350_){
_start:
{
lean_object* v_fst_351_; lean_object* v_snd_352_; uint8_t v___x_353_; 
v_fst_351_ = lean_ctor_get(v_v_349_, 0);
v_snd_352_ = lean_ctor_get(v_v_349_, 1);
v___x_353_ = lp_solar__verify_SolarVerify_Smt_Srt_decEq(v_fst_351_, v_s_350_);
if (v___x_353_ == 0)
{
lean_object* v___x_354_; 
v___x_354_ = lean_box(0);
return v___x_354_;
}
else
{
lean_object* v___x_355_; 
lean_inc(v_snd_352_);
v___x_355_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v___x_355_, 0, v_snd_352_);
return v___x_355_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Val_as___boxed(lean_object* v_v_356_, lean_object* v_s_357_){
_start:
{
lean_object* v_res_358_; 
v_res_358_ = lp_solar__verify_SolarVerify_Smt_Val_as(v_v_356_, v_s_357_);
lean_dec(v_s_357_);
lean_dec_ref(v_v_356_);
return v_res_358_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Val_asList(lean_object* v_x_361_, lean_object* v_x_362_){
_start:
{
if (lean_obj_tag(v_x_361_) == 0)
{
if (lean_obj_tag(v_x_362_) == 0)
{
lean_object* v___x_363_; 
v___x_363_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_Val_asList___closed__0));
return v___x_363_;
}
else
{
lean_object* v___x_364_; 
lean_dec(v_x_362_);
v___x_364_ = lean_box(0);
return v___x_364_;
}
}
else
{
if (lean_obj_tag(v_x_362_) == 1)
{
lean_object* v_head_365_; lean_object* v_tail_366_; lean_object* v_head_367_; lean_object* v_tail_368_; lean_object* v___x_370_; uint8_t v_isShared_371_; uint8_t v_isSharedCheck_388_; 
v_head_365_ = lean_ctor_get(v_x_361_, 0);
v_tail_366_ = lean_ctor_get(v_x_361_, 1);
v_head_367_ = lean_ctor_get(v_x_362_, 0);
v_tail_368_ = lean_ctor_get(v_x_362_, 1);
v_isSharedCheck_388_ = !lean_is_exclusive(v_x_362_);
if (v_isSharedCheck_388_ == 0)
{
v___x_370_ = v_x_362_;
v_isShared_371_ = v_isSharedCheck_388_;
goto v_resetjp_369_;
}
else
{
lean_inc(v_tail_368_);
lean_inc(v_head_367_);
lean_dec(v_x_362_);
v___x_370_ = lean_box(0);
v_isShared_371_ = v_isSharedCheck_388_;
goto v_resetjp_369_;
}
v_resetjp_369_:
{
lean_object* v___x_372_; 
v___x_372_ = lp_solar__verify_SolarVerify_Smt_Val_as(v_head_365_, v_head_367_);
lean_dec(v_head_367_);
if (lean_obj_tag(v___x_372_) == 0)
{
lean_object* v___x_373_; 
lean_del_object(v___x_370_);
lean_dec(v_tail_368_);
v___x_373_ = lean_box(0);
return v___x_373_;
}
else
{
lean_object* v_val_374_; lean_object* v___x_375_; 
v_val_374_ = lean_ctor_get(v___x_372_, 0);
lean_inc(v_val_374_);
lean_dec_ref_known(v___x_372_, 1);
v___x_375_ = lp_solar__verify_SolarVerify_Smt_Val_asList(v_tail_366_, v_tail_368_);
if (lean_obj_tag(v___x_375_) == 0)
{
lean_object* v___x_376_; 
lean_dec(v_val_374_);
lean_del_object(v___x_370_);
v___x_376_ = lean_box(0);
return v___x_376_;
}
else
{
lean_object* v_val_377_; lean_object* v___x_379_; uint8_t v_isShared_380_; uint8_t v_isSharedCheck_387_; 
v_val_377_ = lean_ctor_get(v___x_375_, 0);
v_isSharedCheck_387_ = !lean_is_exclusive(v___x_375_);
if (v_isSharedCheck_387_ == 0)
{
v___x_379_ = v___x_375_;
v_isShared_380_ = v_isSharedCheck_387_;
goto v_resetjp_378_;
}
else
{
lean_inc(v_val_377_);
lean_dec(v___x_375_);
v___x_379_ = lean_box(0);
v_isShared_380_ = v_isSharedCheck_387_;
goto v_resetjp_378_;
}
v_resetjp_378_:
{
lean_object* v___x_382_; 
if (v_isShared_371_ == 0)
{
lean_ctor_set_tag(v___x_370_, 0);
lean_ctor_set(v___x_370_, 1, v_val_377_);
lean_ctor_set(v___x_370_, 0, v_val_374_);
v___x_382_ = v___x_370_;
goto v_reusejp_381_;
}
else
{
lean_object* v_reuseFailAlloc_386_; 
v_reuseFailAlloc_386_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_386_, 0, v_val_374_);
lean_ctor_set(v_reuseFailAlloc_386_, 1, v_val_377_);
v___x_382_ = v_reuseFailAlloc_386_;
goto v_reusejp_381_;
}
v_reusejp_381_:
{
lean_object* v___x_384_; 
if (v_isShared_380_ == 0)
{
lean_ctor_set(v___x_379_, 0, v___x_382_);
v___x_384_ = v___x_379_;
goto v_reusejp_383_;
}
else
{
lean_object* v_reuseFailAlloc_385_; 
v_reuseFailAlloc_385_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_385_, 0, v___x_382_);
v___x_384_ = v_reuseFailAlloc_385_;
goto v_reusejp_383_;
}
v_reusejp_383_:
{
return v___x_384_;
}
}
}
}
}
}
}
else
{
lean_object* v___x_389_; 
lean_dec(v_x_362_);
v___x_389_ = lean_box(0);
return v___x_389_;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Val_asList___boxed(lean_object* v_x_390_, lean_object* v_x_391_){
_start:
{
lean_object* v_res_392_; 
v_res_392_ = lp_solar__verify_SolarVerify_Smt_Val_asList(v_x_390_, v_x_391_);
lean_dec(v_x_390_);
return v_res_392_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_unpack(lean_object* v_x_393_, lean_object* v_x_394_){
_start:
{
if (lean_obj_tag(v_x_393_) == 0)
{
lean_object* v___x_395_; 
lean_dec(v_x_394_);
v___x_395_ = lean_box(0);
return v___x_395_;
}
else
{
lean_object* v_head_396_; lean_object* v_tail_397_; lean_object* v___x_399_; uint8_t v_isShared_400_; uint8_t v_isSharedCheck_414_; 
v_head_396_ = lean_ctor_get(v_x_393_, 0);
v_tail_397_ = lean_ctor_get(v_x_393_, 1);
v_isSharedCheck_414_ = !lean_is_exclusive(v_x_393_);
if (v_isSharedCheck_414_ == 0)
{
v___x_399_ = v_x_393_;
v_isShared_400_ = v_isSharedCheck_414_;
goto v_resetjp_398_;
}
else
{
lean_inc(v_tail_397_);
lean_inc(v_head_396_);
lean_dec(v_x_393_);
v___x_399_ = lean_box(0);
v_isShared_400_ = v_isSharedCheck_414_;
goto v_resetjp_398_;
}
v_resetjp_398_:
{
lean_object* v_fst_401_; lean_object* v_snd_402_; lean_object* v___x_404_; uint8_t v_isShared_405_; uint8_t v_isSharedCheck_413_; 
v_fst_401_ = lean_ctor_get(v_x_394_, 0);
v_snd_402_ = lean_ctor_get(v_x_394_, 1);
v_isSharedCheck_413_ = !lean_is_exclusive(v_x_394_);
if (v_isSharedCheck_413_ == 0)
{
v___x_404_ = v_x_394_;
v_isShared_405_ = v_isSharedCheck_413_;
goto v_resetjp_403_;
}
else
{
lean_inc(v_snd_402_);
lean_inc(v_fst_401_);
lean_dec(v_x_394_);
v___x_404_ = lean_box(0);
v_isShared_405_ = v_isSharedCheck_413_;
goto v_resetjp_403_;
}
v_resetjp_403_:
{
lean_object* v___x_407_; 
if (v_isShared_405_ == 0)
{
lean_ctor_set(v___x_404_, 1, v_fst_401_);
lean_ctor_set(v___x_404_, 0, v_head_396_);
v___x_407_ = v___x_404_;
goto v_reusejp_406_;
}
else
{
lean_object* v_reuseFailAlloc_412_; 
v_reuseFailAlloc_412_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_412_, 0, v_head_396_);
lean_ctor_set(v_reuseFailAlloc_412_, 1, v_fst_401_);
v___x_407_ = v_reuseFailAlloc_412_;
goto v_reusejp_406_;
}
v_reusejp_406_:
{
lean_object* v___x_408_; lean_object* v___x_410_; 
v___x_408_ = lp_solar__verify_SolarVerify_Smt_Srt_unpack(v_tail_397_, v_snd_402_);
if (v_isShared_400_ == 0)
{
lean_ctor_set(v___x_399_, 1, v___x_408_);
lean_ctor_set(v___x_399_, 0, v___x_407_);
v___x_410_ = v___x_399_;
goto v_reusejp_409_;
}
else
{
lean_object* v_reuseFailAlloc_411_; 
v_reuseFailAlloc_411_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v_reuseFailAlloc_411_, 0, v___x_407_);
lean_ctor_set(v_reuseFailAlloc_411_, 1, v___x_408_);
v___x_410_ = v_reuseFailAlloc_411_;
goto v_reusejp_409_;
}
v_reusejp_409_:
{
return v___x_410_;
}
}
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_field(lean_object* v_x_415_, lean_object* v_x_416_, lean_object* v_x_417_){
_start:
{
if (lean_obj_tag(v_x_415_) == 0)
{
lean_object* v___x_418_; 
lean_dec(v_x_417_);
lean_dec(v_x_416_);
v___x_418_ = lean_box(0);
return v___x_418_;
}
else
{
lean_object* v_head_419_; lean_object* v_tail_420_; lean_object* v_fst_421_; lean_object* v_snd_422_; lean_object* v___x_424_; uint8_t v_isShared_425_; uint8_t v_isSharedCheck_435_; 
v_head_419_ = lean_ctor_get(v_x_415_, 0);
v_tail_420_ = lean_ctor_get(v_x_415_, 1);
v_fst_421_ = lean_ctor_get(v_x_416_, 0);
v_snd_422_ = lean_ctor_get(v_x_416_, 1);
v_isSharedCheck_435_ = !lean_is_exclusive(v_x_416_);
if (v_isSharedCheck_435_ == 0)
{
v___x_424_ = v_x_416_;
v_isShared_425_ = v_isSharedCheck_435_;
goto v_resetjp_423_;
}
else
{
lean_inc(v_snd_422_);
lean_inc(v_fst_421_);
lean_dec(v_x_416_);
v___x_424_ = lean_box(0);
v_isShared_425_ = v_isSharedCheck_435_;
goto v_resetjp_423_;
}
v_resetjp_423_:
{
lean_object* v_zero_426_; uint8_t v_isZero_427_; 
v_zero_426_ = lean_unsigned_to_nat(0u);
v_isZero_427_ = lean_nat_dec_eq(v_x_417_, v_zero_426_);
if (v_isZero_427_ == 1)
{
lean_object* v___x_429_; 
lean_dec(v_snd_422_);
lean_dec(v_x_417_);
lean_inc(v_head_419_);
if (v_isShared_425_ == 0)
{
lean_ctor_set(v___x_424_, 1, v_fst_421_);
lean_ctor_set(v___x_424_, 0, v_head_419_);
v___x_429_ = v___x_424_;
goto v_reusejp_428_;
}
else
{
lean_object* v_reuseFailAlloc_431_; 
v_reuseFailAlloc_431_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_431_, 0, v_head_419_);
lean_ctor_set(v_reuseFailAlloc_431_, 1, v_fst_421_);
v___x_429_ = v_reuseFailAlloc_431_;
goto v_reusejp_428_;
}
v_reusejp_428_:
{
lean_object* v___x_430_; 
v___x_430_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v___x_430_, 0, v___x_429_);
return v___x_430_;
}
}
else
{
lean_object* v_one_432_; lean_object* v_n_433_; 
lean_del_object(v___x_424_);
lean_dec(v_fst_421_);
v_one_432_ = lean_unsigned_to_nat(1u);
v_n_433_ = lean_nat_sub(v_x_417_, v_one_432_);
lean_dec(v_x_417_);
v_x_415_ = v_tail_420_;
v_x_416_ = v_snd_422_;
v_x_417_ = v_n_433_;
goto _start;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Srt_field___boxed(lean_object* v_x_436_, lean_object* v_x_437_, lean_object* v_x_438_){
_start:
{
lean_object* v_res_439_; 
v_res_439_ = lp_solar__verify_SolarVerify_Smt_Srt_field(v_x_436_, v_x_437_, v_x_438_);
lean_dec(v_x_436_);
return v_res_439_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Val_bv_x3f(lean_object* v_x_440_){
_start:
{
lean_object* v_fst_441_; 
v_fst_441_ = lean_ctor_get(v_x_440_, 0);
lean_inc(v_fst_441_);
if (lean_obj_tag(v_fst_441_) == 2)
{
lean_object* v_snd_442_; lean_object* v___x_444_; uint8_t v_isShared_445_; uint8_t v_isSharedCheck_457_; 
v_snd_442_ = lean_ctor_get(v_x_440_, 1);
v_isSharedCheck_457_ = !lean_is_exclusive(v_x_440_);
if (v_isSharedCheck_457_ == 0)
{
lean_object* v_unused_458_; 
v_unused_458_ = lean_ctor_get(v_x_440_, 0);
lean_dec(v_unused_458_);
v___x_444_ = v_x_440_;
v_isShared_445_ = v_isSharedCheck_457_;
goto v_resetjp_443_;
}
else
{
lean_inc(v_snd_442_);
lean_dec(v_x_440_);
v___x_444_ = lean_box(0);
v_isShared_445_ = v_isSharedCheck_457_;
goto v_resetjp_443_;
}
v_resetjp_443_:
{
lean_object* v_w_446_; lean_object* v___x_448_; uint8_t v_isShared_449_; uint8_t v_isSharedCheck_456_; 
v_w_446_ = lean_ctor_get(v_fst_441_, 0);
v_isSharedCheck_456_ = !lean_is_exclusive(v_fst_441_);
if (v_isSharedCheck_456_ == 0)
{
v___x_448_ = v_fst_441_;
v_isShared_449_ = v_isSharedCheck_456_;
goto v_resetjp_447_;
}
else
{
lean_inc(v_w_446_);
lean_dec(v_fst_441_);
v___x_448_ = lean_box(0);
v_isShared_449_ = v_isSharedCheck_456_;
goto v_resetjp_447_;
}
v_resetjp_447_:
{
lean_object* v___x_451_; 
if (v_isShared_445_ == 0)
{
lean_ctor_set(v___x_444_, 0, v_w_446_);
v___x_451_ = v___x_444_;
goto v_reusejp_450_;
}
else
{
lean_object* v_reuseFailAlloc_455_; 
v_reuseFailAlloc_455_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_455_, 0, v_w_446_);
lean_ctor_set(v_reuseFailAlloc_455_, 1, v_snd_442_);
v___x_451_ = v_reuseFailAlloc_455_;
goto v_reusejp_450_;
}
v_reusejp_450_:
{
lean_object* v___x_453_; 
if (v_isShared_449_ == 0)
{
lean_ctor_set_tag(v___x_448_, 1);
lean_ctor_set(v___x_448_, 0, v___x_451_);
v___x_453_ = v___x_448_;
goto v_reusejp_452_;
}
else
{
lean_object* v_reuseFailAlloc_454_; 
v_reuseFailAlloc_454_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_454_, 0, v___x_451_);
v___x_453_ = v_reuseFailAlloc_454_;
goto v_reusejp_452_;
}
v_reusejp_452_:
{
return v___x_453_;
}
}
}
}
}
else
{
lean_object* v___x_459_; 
lean_dec(v_fst_441_);
lean_dec_ref(v_x_440_);
v___x_459_ = lean_box(0);
return v___x_459_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_boolOf(lean_object* v_o_460_){
_start:
{
if (lean_obj_tag(v_o_460_) == 0)
{
lean_object* v___x_461_; 
v___x_461_ = lean_box(0);
return v___x_461_;
}
else
{
lean_object* v_val_462_; lean_object* v___x_463_; lean_object* v___x_464_; 
v_val_462_ = lean_ctor_get(v_o_460_, 0);
v___x_463_ = lean_box(0);
v___x_464_ = lp_solar__verify_SolarVerify_Smt_Val_as(v_val_462_, v___x_463_);
return v___x_464_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_boolOf___boxed(lean_object* v_o_465_){
_start:
{
lean_object* v_res_466_; 
v_res_466_ = lp_solar__verify_SolarVerify_Smt_boolOf(v_o_465_);
lean_dec(v_o_465_);
return v_res_466_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ctorIdx(lean_object* v_x_467_){
_start:
{
switch(lean_obj_tag(v_x_467_))
{
case 0:
{
lean_object* v___x_468_; 
v___x_468_ = lean_unsigned_to_nat(0u);
return v___x_468_;
}
case 1:
{
lean_object* v___x_469_; 
v___x_469_ = lean_unsigned_to_nat(1u);
return v___x_469_;
}
case 2:
{
lean_object* v___x_470_; 
v___x_470_ = lean_unsigned_to_nat(2u);
return v___x_470_;
}
case 3:
{
lean_object* v___x_471_; 
v___x_471_ = lean_unsigned_to_nat(3u);
return v___x_471_;
}
case 4:
{
lean_object* v___x_472_; 
v___x_472_ = lean_unsigned_to_nat(4u);
return v___x_472_;
}
case 5:
{
lean_object* v___x_473_; 
v___x_473_ = lean_unsigned_to_nat(5u);
return v___x_473_;
}
case 6:
{
lean_object* v___x_474_; 
v___x_474_ = lean_unsigned_to_nat(6u);
return v___x_474_;
}
case 7:
{
lean_object* v___x_475_; 
v___x_475_ = lean_unsigned_to_nat(7u);
return v___x_475_;
}
case 8:
{
lean_object* v___x_476_; 
v___x_476_ = lean_unsigned_to_nat(8u);
return v___x_476_;
}
case 9:
{
lean_object* v___x_477_; 
v___x_477_ = lean_unsigned_to_nat(9u);
return v___x_477_;
}
case 10:
{
lean_object* v___x_478_; 
v___x_478_ = lean_unsigned_to_nat(10u);
return v___x_478_;
}
case 11:
{
lean_object* v___x_479_; 
v___x_479_ = lean_unsigned_to_nat(11u);
return v___x_479_;
}
case 12:
{
lean_object* v___x_480_; 
v___x_480_ = lean_unsigned_to_nat(12u);
return v___x_480_;
}
case 13:
{
lean_object* v___x_481_; 
v___x_481_ = lean_unsigned_to_nat(13u);
return v___x_481_;
}
case 14:
{
lean_object* v___x_482_; 
v___x_482_ = lean_unsigned_to_nat(14u);
return v___x_482_;
}
case 15:
{
lean_object* v___x_483_; 
v___x_483_ = lean_unsigned_to_nat(15u);
return v___x_483_;
}
case 16:
{
lean_object* v___x_484_; 
v___x_484_ = lean_unsigned_to_nat(16u);
return v___x_484_;
}
case 17:
{
lean_object* v___x_485_; 
v___x_485_ = lean_unsigned_to_nat(17u);
return v___x_485_;
}
case 18:
{
lean_object* v___x_486_; 
v___x_486_ = lean_unsigned_to_nat(18u);
return v___x_486_;
}
case 19:
{
lean_object* v___x_487_; 
v___x_487_ = lean_unsigned_to_nat(19u);
return v___x_487_;
}
case 20:
{
lean_object* v___x_488_; 
v___x_488_ = lean_unsigned_to_nat(20u);
return v___x_488_;
}
case 21:
{
lean_object* v___x_489_; 
v___x_489_ = lean_unsigned_to_nat(21u);
return v___x_489_;
}
case 22:
{
lean_object* v___x_490_; 
v___x_490_ = lean_unsigned_to_nat(22u);
return v___x_490_;
}
case 23:
{
lean_object* v___x_491_; 
v___x_491_ = lean_unsigned_to_nat(23u);
return v___x_491_;
}
case 24:
{
lean_object* v___x_492_; 
v___x_492_ = lean_unsigned_to_nat(24u);
return v___x_492_;
}
case 25:
{
lean_object* v___x_493_; 
v___x_493_ = lean_unsigned_to_nat(25u);
return v___x_493_;
}
case 26:
{
lean_object* v___x_494_; 
v___x_494_ = lean_unsigned_to_nat(26u);
return v___x_494_;
}
case 27:
{
lean_object* v___x_495_; 
v___x_495_ = lean_unsigned_to_nat(27u);
return v___x_495_;
}
case 28:
{
lean_object* v___x_496_; 
v___x_496_ = lean_unsigned_to_nat(28u);
return v___x_496_;
}
case 29:
{
lean_object* v___x_497_; 
v___x_497_ = lean_unsigned_to_nat(29u);
return v___x_497_;
}
case 30:
{
lean_object* v___x_498_; 
v___x_498_ = lean_unsigned_to_nat(30u);
return v___x_498_;
}
case 31:
{
lean_object* v___x_499_; 
v___x_499_ = lean_unsigned_to_nat(31u);
return v___x_499_;
}
case 32:
{
lean_object* v___x_500_; 
v___x_500_ = lean_unsigned_to_nat(32u);
return v___x_500_;
}
case 33:
{
lean_object* v___x_501_; 
v___x_501_ = lean_unsigned_to_nat(33u);
return v___x_501_;
}
case 34:
{
lean_object* v___x_502_; 
v___x_502_ = lean_unsigned_to_nat(34u);
return v___x_502_;
}
case 35:
{
lean_object* v___x_503_; 
v___x_503_ = lean_unsigned_to_nat(35u);
return v___x_503_;
}
case 36:
{
lean_object* v___x_504_; 
v___x_504_ = lean_unsigned_to_nat(36u);
return v___x_504_;
}
case 37:
{
lean_object* v___x_505_; 
v___x_505_ = lean_unsigned_to_nat(37u);
return v___x_505_;
}
case 38:
{
lean_object* v___x_506_; 
v___x_506_ = lean_unsigned_to_nat(38u);
return v___x_506_;
}
case 39:
{
lean_object* v___x_507_; 
v___x_507_ = lean_unsigned_to_nat(39u);
return v___x_507_;
}
case 40:
{
lean_object* v___x_508_; 
v___x_508_ = lean_unsigned_to_nat(40u);
return v___x_508_;
}
case 41:
{
lean_object* v___x_509_; 
v___x_509_ = lean_unsigned_to_nat(41u);
return v___x_509_;
}
case 42:
{
lean_object* v___x_510_; 
v___x_510_ = lean_unsigned_to_nat(42u);
return v___x_510_;
}
case 43:
{
lean_object* v___x_511_; 
v___x_511_ = lean_unsigned_to_nat(43u);
return v___x_511_;
}
case 44:
{
lean_object* v___x_512_; 
v___x_512_ = lean_unsigned_to_nat(44u);
return v___x_512_;
}
case 45:
{
lean_object* v___x_513_; 
v___x_513_ = lean_unsigned_to_nat(45u);
return v___x_513_;
}
case 46:
{
lean_object* v___x_514_; 
v___x_514_ = lean_unsigned_to_nat(46u);
return v___x_514_;
}
case 47:
{
lean_object* v___x_515_; 
v___x_515_ = lean_unsigned_to_nat(47u);
return v___x_515_;
}
case 48:
{
lean_object* v___x_516_; 
v___x_516_ = lean_unsigned_to_nat(48u);
return v___x_516_;
}
default: 
{
lean_object* v___x_517_; 
v___x_517_ = lean_unsigned_to_nat(49u);
return v___x_517_;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ctorIdx___boxed(lean_object* v_x_518_){
_start:
{
lean_object* v_res_519_; 
v_res_519_ = lp_solar__verify_SolarVerify_Smt_Op_ctorIdx(v_x_518_);
lean_dec(v_x_518_);
return v_res_519_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(lean_object* v_t_520_, lean_object* v_k_521_){
_start:
{
switch(lean_obj_tag(v_t_520_))
{
case 21:
{
lean_object* v_w_522_; lean_object* v___x_523_; 
v_w_522_ = lean_ctor_get(v_t_520_, 0);
lean_inc(v_w_522_);
lean_dec_ref_known(v_t_520_, 1);
v___x_523_ = lean_apply_1(v_k_521_, v_w_522_);
return v___x_523_;
}
case 47:
{
lean_object* v_hi_524_; lean_object* v_lo_525_; lean_object* v___x_526_; 
v_hi_524_ = lean_ctor_get(v_t_520_, 0);
lean_inc(v_hi_524_);
v_lo_525_ = lean_ctor_get(v_t_520_, 1);
lean_inc(v_lo_525_);
lean_dec_ref_known(v_t_520_, 2);
v___x_526_ = lean_apply_2(v_k_521_, v_hi_524_, v_lo_525_);
return v___x_526_;
}
case 48:
{
lean_object* v_n_527_; lean_object* v___x_528_; 
v_n_527_ = lean_ctor_get(v_t_520_, 0);
lean_inc(v_n_527_);
lean_dec_ref_known(v_t_520_, 1);
v___x_528_ = lean_apply_1(v_k_521_, v_n_527_);
return v___x_528_;
}
case 49:
{
lean_object* v_n_529_; lean_object* v___x_530_; 
v_n_529_ = lean_ctor_get(v_t_520_, 0);
lean_inc(v_n_529_);
lean_dec_ref_known(v_t_520_, 1);
v___x_530_ = lean_apply_1(v_k_521_, v_n_529_);
return v___x_530_;
}
default: 
{
lean_dec(v_t_520_);
return v_k_521_;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ctorElim(lean_object* v_motive_531_, lean_object* v_ctorIdx_532_, lean_object* v_t_533_, lean_object* v_h_534_, lean_object* v_k_535_){
_start:
{
lean_object* v___x_536_; 
v___x_536_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_533_, v_k_535_);
return v___x_536_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ctorElim___boxed(lean_object* v_motive_537_, lean_object* v_ctorIdx_538_, lean_object* v_t_539_, lean_object* v_h_540_, lean_object* v_k_541_){
_start:
{
lean_object* v_res_542_; 
v_res_542_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim(v_motive_537_, v_ctorIdx_538_, v_t_539_, v_h_540_, v_k_541_);
lean_dec(v_ctorIdx_538_);
return v_res_542_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_not_elim___redArg(lean_object* v_t_543_, lean_object* v_not_544_){
_start:
{
lean_object* v___x_545_; 
v___x_545_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_543_, v_not_544_);
return v___x_545_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_not_elim(lean_object* v_motive_546_, lean_object* v_t_547_, lean_object* v_h_548_, lean_object* v_not_549_){
_start:
{
lean_object* v___x_550_; 
v___x_550_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_547_, v_not_549_);
return v___x_550_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_and_elim___redArg(lean_object* v_t_551_, lean_object* v_and_552_){
_start:
{
lean_object* v___x_553_; 
v___x_553_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_551_, v_and_552_);
return v___x_553_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_and_elim(lean_object* v_motive_554_, lean_object* v_t_555_, lean_object* v_h_556_, lean_object* v_and_557_){
_start:
{
lean_object* v___x_558_; 
v___x_558_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_555_, v_and_557_);
return v___x_558_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_or_elim___redArg(lean_object* v_t_559_, lean_object* v_or_560_){
_start:
{
lean_object* v___x_561_; 
v___x_561_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_559_, v_or_560_);
return v___x_561_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_or_elim(lean_object* v_motive_562_, lean_object* v_t_563_, lean_object* v_h_564_, lean_object* v_or_565_){
_start:
{
lean_object* v___x_566_; 
v___x_566_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_563_, v_or_565_);
return v___x_566_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_xor_elim___redArg(lean_object* v_t_567_, lean_object* v_xor_568_){
_start:
{
lean_object* v___x_569_; 
v___x_569_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_567_, v_xor_568_);
return v___x_569_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_xor_elim(lean_object* v_motive_570_, lean_object* v_t_571_, lean_object* v_h_572_, lean_object* v_xor_573_){
_start:
{
lean_object* v___x_574_; 
v___x_574_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_571_, v_xor_573_);
return v___x_574_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_implies_elim___redArg(lean_object* v_t_575_, lean_object* v_implies_576_){
_start:
{
lean_object* v___x_577_; 
v___x_577_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_575_, v_implies_576_);
return v___x_577_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_implies_elim(lean_object* v_motive_578_, lean_object* v_t_579_, lean_object* v_h_580_, lean_object* v_implies_581_){
_start:
{
lean_object* v___x_582_; 
v___x_582_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_579_, v_implies_581_);
return v___x_582_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_eq_elim___redArg(lean_object* v_t_583_, lean_object* v_eq_584_){
_start:
{
lean_object* v___x_585_; 
v___x_585_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_583_, v_eq_584_);
return v___x_585_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_eq_elim(lean_object* v_motive_586_, lean_object* v_t_587_, lean_object* v_h_588_, lean_object* v_eq_589_){
_start:
{
lean_object* v___x_590_; 
v___x_590_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_587_, v_eq_589_);
return v___x_590_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_distinct_elim___redArg(lean_object* v_t_591_, lean_object* v_distinct_592_){
_start:
{
lean_object* v___x_593_; 
v___x_593_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_591_, v_distinct_592_);
return v___x_593_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_distinct_elim(lean_object* v_motive_594_, lean_object* v_t_595_, lean_object* v_h_596_, lean_object* v_distinct_597_){
_start:
{
lean_object* v___x_598_; 
v___x_598_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_595_, v_distinct_597_);
return v___x_598_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ite_elim___redArg(lean_object* v_t_599_, lean_object* v_ite_600_){
_start:
{
lean_object* v___x_601_; 
v___x_601_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_599_, v_ite_600_);
return v___x_601_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ite_elim(lean_object* v_motive_602_, lean_object* v_t_603_, lean_object* v_h_604_, lean_object* v_ite_605_){
_start:
{
lean_object* v___x_606_; 
v___x_606_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_603_, v_ite_605_);
return v___x_606_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_add_elim___redArg(lean_object* v_t_607_, lean_object* v_add_608_){
_start:
{
lean_object* v___x_609_; 
v___x_609_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_607_, v_add_608_);
return v___x_609_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_add_elim(lean_object* v_motive_610_, lean_object* v_t_611_, lean_object* v_h_612_, lean_object* v_add_613_){
_start:
{
lean_object* v___x_614_; 
v___x_614_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_611_, v_add_613_);
return v___x_614_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_sub_elim___redArg(lean_object* v_t_615_, lean_object* v_sub_616_){
_start:
{
lean_object* v___x_617_; 
v___x_617_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_615_, v_sub_616_);
return v___x_617_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_sub_elim(lean_object* v_motive_618_, lean_object* v_t_619_, lean_object* v_h_620_, lean_object* v_sub_621_){
_start:
{
lean_object* v___x_622_; 
v___x_622_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_619_, v_sub_621_);
return v___x_622_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_mul_elim___redArg(lean_object* v_t_623_, lean_object* v_mul_624_){
_start:
{
lean_object* v___x_625_; 
v___x_625_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_623_, v_mul_624_);
return v___x_625_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_mul_elim(lean_object* v_motive_626_, lean_object* v_t_627_, lean_object* v_h_628_, lean_object* v_mul_629_){
_start:
{
lean_object* v___x_630_; 
v___x_630_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_627_, v_mul_629_);
return v___x_630_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_div_elim___redArg(lean_object* v_t_631_, lean_object* v_div_632_){
_start:
{
lean_object* v___x_633_; 
v___x_633_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_631_, v_div_632_);
return v___x_633_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_div_elim(lean_object* v_motive_634_, lean_object* v_t_635_, lean_object* v_h_636_, lean_object* v_div_637_){
_start:
{
lean_object* v___x_638_; 
v___x_638_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_635_, v_div_637_);
return v___x_638_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_mod_elim___redArg(lean_object* v_t_639_, lean_object* v_mod_640_){
_start:
{
lean_object* v___x_641_; 
v___x_641_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_639_, v_mod_640_);
return v___x_641_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_mod_elim(lean_object* v_motive_642_, lean_object* v_t_643_, lean_object* v_h_644_, lean_object* v_mod_645_){
_start:
{
lean_object* v___x_646_; 
v___x_646_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_643_, v_mod_645_);
return v___x_646_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_abs_elim___redArg(lean_object* v_t_647_, lean_object* v_abs_648_){
_start:
{
lean_object* v___x_649_; 
v___x_649_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_647_, v_abs_648_);
return v___x_649_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_abs_elim(lean_object* v_motive_650_, lean_object* v_t_651_, lean_object* v_h_652_, lean_object* v_abs_653_){
_start:
{
lean_object* v___x_654_; 
v___x_654_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_651_, v_abs_653_);
return v___x_654_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_lt_elim___redArg(lean_object* v_t_655_, lean_object* v_lt_656_){
_start:
{
lean_object* v___x_657_; 
v___x_657_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_655_, v_lt_656_);
return v___x_657_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_lt_elim(lean_object* v_motive_658_, lean_object* v_t_659_, lean_object* v_h_660_, lean_object* v_lt_661_){
_start:
{
lean_object* v___x_662_; 
v___x_662_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_659_, v_lt_661_);
return v___x_662_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_le_elim___redArg(lean_object* v_t_663_, lean_object* v_le_664_){
_start:
{
lean_object* v___x_665_; 
v___x_665_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_663_, v_le_664_);
return v___x_665_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_le_elim(lean_object* v_motive_666_, lean_object* v_t_667_, lean_object* v_h_668_, lean_object* v_le_669_){
_start:
{
lean_object* v___x_670_; 
v___x_670_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_667_, v_le_669_);
return v___x_670_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_gt_elim___redArg(lean_object* v_t_671_, lean_object* v_gt_672_){
_start:
{
lean_object* v___x_673_; 
v___x_673_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_671_, v_gt_672_);
return v___x_673_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_gt_elim(lean_object* v_motive_674_, lean_object* v_t_675_, lean_object* v_h_676_, lean_object* v_gt_677_){
_start:
{
lean_object* v___x_678_; 
v___x_678_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_675_, v_gt_677_);
return v___x_678_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ge_elim___redArg(lean_object* v_t_679_, lean_object* v_ge_680_){
_start:
{
lean_object* v___x_681_; 
v___x_681_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_679_, v_ge_680_);
return v___x_681_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_ge_elim(lean_object* v_motive_682_, lean_object* v_t_683_, lean_object* v_h_684_, lean_object* v_ge_685_){
_start:
{
lean_object* v___x_686_; 
v___x_686_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_683_, v_ge_685_);
return v___x_686_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_select_elim___redArg(lean_object* v_t_687_, lean_object* v_select_688_){
_start:
{
lean_object* v___x_689_; 
v___x_689_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_687_, v_select_688_);
return v___x_689_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_select_elim(lean_object* v_motive_690_, lean_object* v_t_691_, lean_object* v_h_692_, lean_object* v_select_693_){
_start:
{
lean_object* v___x_694_; 
v___x_694_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_691_, v_select_693_);
return v___x_694_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_store_elim___redArg(lean_object* v_t_695_, lean_object* v_store_696_){
_start:
{
lean_object* v___x_697_; 
v___x_697_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_695_, v_store_696_);
return v___x_697_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_store_elim(lean_object* v_motive_698_, lean_object* v_t_699_, lean_object* v_h_700_, lean_object* v_store_701_){
_start:
{
lean_object* v___x_702_; 
v___x_702_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_699_, v_store_701_);
return v___x_702_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bv2nat_elim___redArg(lean_object* v_t_703_, lean_object* v_bv2nat_704_){
_start:
{
lean_object* v___x_705_; 
v___x_705_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_703_, v_bv2nat_704_);
return v___x_705_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bv2nat_elim(lean_object* v_motive_706_, lean_object* v_t_707_, lean_object* v_h_708_, lean_object* v_bv2nat_709_){
_start:
{
lean_object* v___x_710_; 
v___x_710_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_707_, v_bv2nat_709_);
return v___x_710_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_int2bv_elim___redArg(lean_object* v_t_711_, lean_object* v_int2bv_712_){
_start:
{
lean_object* v___x_713_; 
v___x_713_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_711_, v_int2bv_712_);
return v___x_713_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_int2bv_elim(lean_object* v_motive_714_, lean_object* v_t_715_, lean_object* v_h_716_, lean_object* v_int2bv_717_){
_start:
{
lean_object* v___x_718_; 
v___x_718_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_715_, v_int2bv_717_);
return v___x_718_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvadd_elim___redArg(lean_object* v_t_719_, lean_object* v_bvadd_720_){
_start:
{
lean_object* v___x_721_; 
v___x_721_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_719_, v_bvadd_720_);
return v___x_721_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvadd_elim(lean_object* v_motive_722_, lean_object* v_t_723_, lean_object* v_h_724_, lean_object* v_bvadd_725_){
_start:
{
lean_object* v___x_726_; 
v___x_726_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_723_, v_bvadd_725_);
return v___x_726_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsub_elim___redArg(lean_object* v_t_727_, lean_object* v_bvsub_728_){
_start:
{
lean_object* v___x_729_; 
v___x_729_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_727_, v_bvsub_728_);
return v___x_729_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsub_elim(lean_object* v_motive_730_, lean_object* v_t_731_, lean_object* v_h_732_, lean_object* v_bvsub_733_){
_start:
{
lean_object* v___x_734_; 
v___x_734_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_731_, v_bvsub_733_);
return v___x_734_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvmul_elim___redArg(lean_object* v_t_735_, lean_object* v_bvmul_736_){
_start:
{
lean_object* v___x_737_; 
v___x_737_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_735_, v_bvmul_736_);
return v___x_737_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvmul_elim(lean_object* v_motive_738_, lean_object* v_t_739_, lean_object* v_h_740_, lean_object* v_bvmul_741_){
_start:
{
lean_object* v___x_742_; 
v___x_742_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_739_, v_bvmul_741_);
return v___x_742_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvudiv_elim___redArg(lean_object* v_t_743_, lean_object* v_bvudiv_744_){
_start:
{
lean_object* v___x_745_; 
v___x_745_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_743_, v_bvudiv_744_);
return v___x_745_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvudiv_elim(lean_object* v_motive_746_, lean_object* v_t_747_, lean_object* v_h_748_, lean_object* v_bvudiv_749_){
_start:
{
lean_object* v___x_750_; 
v___x_750_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_747_, v_bvudiv_749_);
return v___x_750_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvurem_elim___redArg(lean_object* v_t_751_, lean_object* v_bvurem_752_){
_start:
{
lean_object* v___x_753_; 
v___x_753_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_751_, v_bvurem_752_);
return v___x_753_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvurem_elim(lean_object* v_motive_754_, lean_object* v_t_755_, lean_object* v_h_756_, lean_object* v_bvurem_757_){
_start:
{
lean_object* v___x_758_; 
v___x_758_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_755_, v_bvurem_757_);
return v___x_758_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsdiv_elim___redArg(lean_object* v_t_759_, lean_object* v_bvsdiv_760_){
_start:
{
lean_object* v___x_761_; 
v___x_761_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_759_, v_bvsdiv_760_);
return v___x_761_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsdiv_elim(lean_object* v_motive_762_, lean_object* v_t_763_, lean_object* v_h_764_, lean_object* v_bvsdiv_765_){
_start:
{
lean_object* v___x_766_; 
v___x_766_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_763_, v_bvsdiv_765_);
return v___x_766_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsrem_elim___redArg(lean_object* v_t_767_, lean_object* v_bvsrem_768_){
_start:
{
lean_object* v___x_769_; 
v___x_769_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_767_, v_bvsrem_768_);
return v___x_769_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsrem_elim(lean_object* v_motive_770_, lean_object* v_t_771_, lean_object* v_h_772_, lean_object* v_bvsrem_773_){
_start:
{
lean_object* v___x_774_; 
v___x_774_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_771_, v_bvsrem_773_);
return v___x_774_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsmod_elim___redArg(lean_object* v_t_775_, lean_object* v_bvsmod_776_){
_start:
{
lean_object* v___x_777_; 
v___x_777_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_775_, v_bvsmod_776_);
return v___x_777_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsmod_elim(lean_object* v_motive_778_, lean_object* v_t_779_, lean_object* v_h_780_, lean_object* v_bvsmod_781_){
_start:
{
lean_object* v___x_782_; 
v___x_782_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_779_, v_bvsmod_781_);
return v___x_782_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvneg_elim___redArg(lean_object* v_t_783_, lean_object* v_bvneg_784_){
_start:
{
lean_object* v___x_785_; 
v___x_785_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_783_, v_bvneg_784_);
return v___x_785_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvneg_elim(lean_object* v_motive_786_, lean_object* v_t_787_, lean_object* v_h_788_, lean_object* v_bvneg_789_){
_start:
{
lean_object* v___x_790_; 
v___x_790_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_787_, v_bvneg_789_);
return v___x_790_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvand_elim___redArg(lean_object* v_t_791_, lean_object* v_bvand_792_){
_start:
{
lean_object* v___x_793_; 
v___x_793_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_791_, v_bvand_792_);
return v___x_793_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvand_elim(lean_object* v_motive_794_, lean_object* v_t_795_, lean_object* v_h_796_, lean_object* v_bvand_797_){
_start:
{
lean_object* v___x_798_; 
v___x_798_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_795_, v_bvand_797_);
return v___x_798_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvor_elim___redArg(lean_object* v_t_799_, lean_object* v_bvor_800_){
_start:
{
lean_object* v___x_801_; 
v___x_801_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_799_, v_bvor_800_);
return v___x_801_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvor_elim(lean_object* v_motive_802_, lean_object* v_t_803_, lean_object* v_h_804_, lean_object* v_bvor_805_){
_start:
{
lean_object* v___x_806_; 
v___x_806_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_803_, v_bvor_805_);
return v___x_806_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvxor_elim___redArg(lean_object* v_t_807_, lean_object* v_bvxor_808_){
_start:
{
lean_object* v___x_809_; 
v___x_809_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_807_, v_bvxor_808_);
return v___x_809_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvxor_elim(lean_object* v_motive_810_, lean_object* v_t_811_, lean_object* v_h_812_, lean_object* v_bvxor_813_){
_start:
{
lean_object* v___x_814_; 
v___x_814_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_811_, v_bvxor_813_);
return v___x_814_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvnot_elim___redArg(lean_object* v_t_815_, lean_object* v_bvnot_816_){
_start:
{
lean_object* v___x_817_; 
v___x_817_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_815_, v_bvnot_816_);
return v___x_817_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvnot_elim(lean_object* v_motive_818_, lean_object* v_t_819_, lean_object* v_h_820_, lean_object* v_bvnot_821_){
_start:
{
lean_object* v___x_822_; 
v___x_822_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_819_, v_bvnot_821_);
return v___x_822_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvshl_elim___redArg(lean_object* v_t_823_, lean_object* v_bvshl_824_){
_start:
{
lean_object* v___x_825_; 
v___x_825_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_823_, v_bvshl_824_);
return v___x_825_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvshl_elim(lean_object* v_motive_826_, lean_object* v_t_827_, lean_object* v_h_828_, lean_object* v_bvshl_829_){
_start:
{
lean_object* v___x_830_; 
v___x_830_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_827_, v_bvshl_829_);
return v___x_830_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvlshr_elim___redArg(lean_object* v_t_831_, lean_object* v_bvlshr_832_){
_start:
{
lean_object* v___x_833_; 
v___x_833_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_831_, v_bvlshr_832_);
return v___x_833_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvlshr_elim(lean_object* v_motive_834_, lean_object* v_t_835_, lean_object* v_h_836_, lean_object* v_bvlshr_837_){
_start:
{
lean_object* v___x_838_; 
v___x_838_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_835_, v_bvlshr_837_);
return v___x_838_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvashr_elim___redArg(lean_object* v_t_839_, lean_object* v_bvashr_840_){
_start:
{
lean_object* v___x_841_; 
v___x_841_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_839_, v_bvashr_840_);
return v___x_841_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvashr_elim(lean_object* v_motive_842_, lean_object* v_t_843_, lean_object* v_h_844_, lean_object* v_bvashr_845_){
_start:
{
lean_object* v___x_846_; 
v___x_846_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_843_, v_bvashr_845_);
return v___x_846_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvult_elim___redArg(lean_object* v_t_847_, lean_object* v_bvult_848_){
_start:
{
lean_object* v___x_849_; 
v___x_849_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_847_, v_bvult_848_);
return v___x_849_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvult_elim(lean_object* v_motive_850_, lean_object* v_t_851_, lean_object* v_h_852_, lean_object* v_bvult_853_){
_start:
{
lean_object* v___x_854_; 
v___x_854_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_851_, v_bvult_853_);
return v___x_854_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvule_elim___redArg(lean_object* v_t_855_, lean_object* v_bvule_856_){
_start:
{
lean_object* v___x_857_; 
v___x_857_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_855_, v_bvule_856_);
return v___x_857_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvule_elim(lean_object* v_motive_858_, lean_object* v_t_859_, lean_object* v_h_860_, lean_object* v_bvule_861_){
_start:
{
lean_object* v___x_862_; 
v___x_862_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_859_, v_bvule_861_);
return v___x_862_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvugt_elim___redArg(lean_object* v_t_863_, lean_object* v_bvugt_864_){
_start:
{
lean_object* v___x_865_; 
v___x_865_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_863_, v_bvugt_864_);
return v___x_865_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvugt_elim(lean_object* v_motive_866_, lean_object* v_t_867_, lean_object* v_h_868_, lean_object* v_bvugt_869_){
_start:
{
lean_object* v___x_870_; 
v___x_870_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_867_, v_bvugt_869_);
return v___x_870_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvuge_elim___redArg(lean_object* v_t_871_, lean_object* v_bvuge_872_){
_start:
{
lean_object* v___x_873_; 
v___x_873_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_871_, v_bvuge_872_);
return v___x_873_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvuge_elim(lean_object* v_motive_874_, lean_object* v_t_875_, lean_object* v_h_876_, lean_object* v_bvuge_877_){
_start:
{
lean_object* v___x_878_; 
v___x_878_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_875_, v_bvuge_877_);
return v___x_878_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvslt_elim___redArg(lean_object* v_t_879_, lean_object* v_bvslt_880_){
_start:
{
lean_object* v___x_881_; 
v___x_881_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_879_, v_bvslt_880_);
return v___x_881_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvslt_elim(lean_object* v_motive_882_, lean_object* v_t_883_, lean_object* v_h_884_, lean_object* v_bvslt_885_){
_start:
{
lean_object* v___x_886_; 
v___x_886_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_883_, v_bvslt_885_);
return v___x_886_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsle_elim___redArg(lean_object* v_t_887_, lean_object* v_bvsle_888_){
_start:
{
lean_object* v___x_889_; 
v___x_889_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_887_, v_bvsle_888_);
return v___x_889_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsle_elim(lean_object* v_motive_890_, lean_object* v_t_891_, lean_object* v_h_892_, lean_object* v_bvsle_893_){
_start:
{
lean_object* v___x_894_; 
v___x_894_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_891_, v_bvsle_893_);
return v___x_894_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsgt_elim___redArg(lean_object* v_t_895_, lean_object* v_bvsgt_896_){
_start:
{
lean_object* v___x_897_; 
v___x_897_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_895_, v_bvsgt_896_);
return v___x_897_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsgt_elim(lean_object* v_motive_898_, lean_object* v_t_899_, lean_object* v_h_900_, lean_object* v_bvsgt_901_){
_start:
{
lean_object* v___x_902_; 
v___x_902_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_899_, v_bvsgt_901_);
return v___x_902_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsge_elim___redArg(lean_object* v_t_903_, lean_object* v_bvsge_904_){
_start:
{
lean_object* v___x_905_; 
v___x_905_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_903_, v_bvsge_904_);
return v___x_905_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_bvsge_elim(lean_object* v_motive_906_, lean_object* v_t_907_, lean_object* v_h_908_, lean_object* v_bvsge_909_){
_start:
{
lean_object* v___x_910_; 
v___x_910_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_907_, v_bvsge_909_);
return v___x_910_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_concat_elim___redArg(lean_object* v_t_911_, lean_object* v_concat_912_){
_start:
{
lean_object* v___x_913_; 
v___x_913_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_911_, v_concat_912_);
return v___x_913_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_concat_elim(lean_object* v_motive_914_, lean_object* v_t_915_, lean_object* v_h_916_, lean_object* v_concat_917_){
_start:
{
lean_object* v___x_918_; 
v___x_918_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_915_, v_concat_917_);
return v___x_918_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_extract_elim___redArg(lean_object* v_t_919_, lean_object* v_extract_920_){
_start:
{
lean_object* v___x_921_; 
v___x_921_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_919_, v_extract_920_);
return v___x_921_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_extract_elim(lean_object* v_motive_922_, lean_object* v_t_923_, lean_object* v_h_924_, lean_object* v_extract_925_){
_start:
{
lean_object* v___x_926_; 
v___x_926_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_923_, v_extract_925_);
return v___x_926_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_zeroExtend_elim___redArg(lean_object* v_t_927_, lean_object* v_zeroExtend_928_){
_start:
{
lean_object* v___x_929_; 
v___x_929_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_927_, v_zeroExtend_928_);
return v___x_929_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_zeroExtend_elim(lean_object* v_motive_930_, lean_object* v_t_931_, lean_object* v_h_932_, lean_object* v_zeroExtend_933_){
_start:
{
lean_object* v___x_934_; 
v___x_934_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_931_, v_zeroExtend_933_);
return v___x_934_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_signExtend_elim___redArg(lean_object* v_t_935_, lean_object* v_signExtend_936_){
_start:
{
lean_object* v___x_937_; 
v___x_937_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_935_, v_signExtend_936_);
return v___x_937_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Op_signExtend_elim(lean_object* v_motive_938_, lean_object* v_t_939_, lean_object* v_h_940_, lean_object* v_signExtend_941_){
_start:
{
lean_object* v___x_942_; 
v___x_942_ = lp_solar__verify_SolarVerify_Smt_Op_ctorElim___redArg(v_t_939_, v_signExtend_941_);
return v___x_942_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr(lean_object* v_x_1105_, lean_object* v_prec_1106_){
_start:
{
lean_object* v___y_1108_; lean_object* v___y_1115_; lean_object* v___y_1122_; lean_object* v___y_1129_; lean_object* v___y_1136_; lean_object* v___y_1143_; lean_object* v___y_1150_; lean_object* v___y_1157_; lean_object* v___y_1164_; lean_object* v___y_1171_; lean_object* v___y_1178_; lean_object* v___y_1185_; lean_object* v___y_1192_; lean_object* v___y_1199_; lean_object* v___y_1206_; lean_object* v___y_1213_; lean_object* v___y_1220_; lean_object* v___y_1227_; lean_object* v___y_1234_; lean_object* v___y_1241_; lean_object* v___y_1248_; lean_object* v___y_1255_; lean_object* v___y_1262_; lean_object* v___y_1269_; lean_object* v___y_1276_; lean_object* v___y_1283_; lean_object* v___y_1290_; lean_object* v___y_1297_; lean_object* v___y_1304_; lean_object* v___y_1311_; lean_object* v___y_1318_; lean_object* v___y_1325_; lean_object* v___y_1332_; lean_object* v___y_1339_; lean_object* v___y_1346_; lean_object* v___y_1353_; lean_object* v___y_1360_; lean_object* v___y_1367_; lean_object* v___y_1374_; lean_object* v___y_1381_; lean_object* v___y_1388_; lean_object* v___y_1395_; lean_object* v___y_1402_; lean_object* v___y_1409_; lean_object* v___y_1416_; lean_object* v___y_1423_; 
switch(lean_obj_tag(v_x_1105_))
{
case 0:
{
lean_object* v___x_1429_; uint8_t v___x_1430_; 
v___x_1429_ = lean_unsigned_to_nat(1024u);
v___x_1430_ = lean_nat_dec_le(v___x_1429_, v_prec_1106_);
if (v___x_1430_ == 0)
{
lean_object* v___x_1431_; 
v___x_1431_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1283_ = v___x_1431_;
goto v___jp_1282_;
}
else
{
lean_object* v___x_1432_; 
v___x_1432_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1283_ = v___x_1432_;
goto v___jp_1282_;
}
}
case 1:
{
lean_object* v___x_1433_; uint8_t v___x_1434_; 
v___x_1433_ = lean_unsigned_to_nat(1024u);
v___x_1434_ = lean_nat_dec_le(v___x_1433_, v_prec_1106_);
if (v___x_1434_ == 0)
{
lean_object* v___x_1435_; 
v___x_1435_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1290_ = v___x_1435_;
goto v___jp_1289_;
}
else
{
lean_object* v___x_1436_; 
v___x_1436_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1290_ = v___x_1436_;
goto v___jp_1289_;
}
}
case 2:
{
lean_object* v___x_1437_; uint8_t v___x_1438_; 
v___x_1437_ = lean_unsigned_to_nat(1024u);
v___x_1438_ = lean_nat_dec_le(v___x_1437_, v_prec_1106_);
if (v___x_1438_ == 0)
{
lean_object* v___x_1439_; 
v___x_1439_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1297_ = v___x_1439_;
goto v___jp_1296_;
}
else
{
lean_object* v___x_1440_; 
v___x_1440_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1297_ = v___x_1440_;
goto v___jp_1296_;
}
}
case 3:
{
lean_object* v___x_1441_; uint8_t v___x_1442_; 
v___x_1441_ = lean_unsigned_to_nat(1024u);
v___x_1442_ = lean_nat_dec_le(v___x_1441_, v_prec_1106_);
if (v___x_1442_ == 0)
{
lean_object* v___x_1443_; 
v___x_1443_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1304_ = v___x_1443_;
goto v___jp_1303_;
}
else
{
lean_object* v___x_1444_; 
v___x_1444_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1304_ = v___x_1444_;
goto v___jp_1303_;
}
}
case 4:
{
lean_object* v___x_1445_; uint8_t v___x_1446_; 
v___x_1445_ = lean_unsigned_to_nat(1024u);
v___x_1446_ = lean_nat_dec_le(v___x_1445_, v_prec_1106_);
if (v___x_1446_ == 0)
{
lean_object* v___x_1447_; 
v___x_1447_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1311_ = v___x_1447_;
goto v___jp_1310_;
}
else
{
lean_object* v___x_1448_; 
v___x_1448_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1311_ = v___x_1448_;
goto v___jp_1310_;
}
}
case 5:
{
lean_object* v___x_1449_; uint8_t v___x_1450_; 
v___x_1449_ = lean_unsigned_to_nat(1024u);
v___x_1450_ = lean_nat_dec_le(v___x_1449_, v_prec_1106_);
if (v___x_1450_ == 0)
{
lean_object* v___x_1451_; 
v___x_1451_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1318_ = v___x_1451_;
goto v___jp_1317_;
}
else
{
lean_object* v___x_1452_; 
v___x_1452_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1318_ = v___x_1452_;
goto v___jp_1317_;
}
}
case 6:
{
lean_object* v___x_1453_; uint8_t v___x_1454_; 
v___x_1453_ = lean_unsigned_to_nat(1024u);
v___x_1454_ = lean_nat_dec_le(v___x_1453_, v_prec_1106_);
if (v___x_1454_ == 0)
{
lean_object* v___x_1455_; 
v___x_1455_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1325_ = v___x_1455_;
goto v___jp_1324_;
}
else
{
lean_object* v___x_1456_; 
v___x_1456_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1325_ = v___x_1456_;
goto v___jp_1324_;
}
}
case 7:
{
lean_object* v___x_1457_; uint8_t v___x_1458_; 
v___x_1457_ = lean_unsigned_to_nat(1024u);
v___x_1458_ = lean_nat_dec_le(v___x_1457_, v_prec_1106_);
if (v___x_1458_ == 0)
{
lean_object* v___x_1459_; 
v___x_1459_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1332_ = v___x_1459_;
goto v___jp_1331_;
}
else
{
lean_object* v___x_1460_; 
v___x_1460_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1332_ = v___x_1460_;
goto v___jp_1331_;
}
}
case 8:
{
lean_object* v___x_1461_; uint8_t v___x_1462_; 
v___x_1461_ = lean_unsigned_to_nat(1024u);
v___x_1462_ = lean_nat_dec_le(v___x_1461_, v_prec_1106_);
if (v___x_1462_ == 0)
{
lean_object* v___x_1463_; 
v___x_1463_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1339_ = v___x_1463_;
goto v___jp_1338_;
}
else
{
lean_object* v___x_1464_; 
v___x_1464_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1339_ = v___x_1464_;
goto v___jp_1338_;
}
}
case 9:
{
lean_object* v___x_1465_; uint8_t v___x_1466_; 
v___x_1465_ = lean_unsigned_to_nat(1024u);
v___x_1466_ = lean_nat_dec_le(v___x_1465_, v_prec_1106_);
if (v___x_1466_ == 0)
{
lean_object* v___x_1467_; 
v___x_1467_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1346_ = v___x_1467_;
goto v___jp_1345_;
}
else
{
lean_object* v___x_1468_; 
v___x_1468_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1346_ = v___x_1468_;
goto v___jp_1345_;
}
}
case 10:
{
lean_object* v___x_1469_; uint8_t v___x_1470_; 
v___x_1469_ = lean_unsigned_to_nat(1024u);
v___x_1470_ = lean_nat_dec_le(v___x_1469_, v_prec_1106_);
if (v___x_1470_ == 0)
{
lean_object* v___x_1471_; 
v___x_1471_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1353_ = v___x_1471_;
goto v___jp_1352_;
}
else
{
lean_object* v___x_1472_; 
v___x_1472_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1353_ = v___x_1472_;
goto v___jp_1352_;
}
}
case 11:
{
lean_object* v___x_1473_; uint8_t v___x_1474_; 
v___x_1473_ = lean_unsigned_to_nat(1024u);
v___x_1474_ = lean_nat_dec_le(v___x_1473_, v_prec_1106_);
if (v___x_1474_ == 0)
{
lean_object* v___x_1475_; 
v___x_1475_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1360_ = v___x_1475_;
goto v___jp_1359_;
}
else
{
lean_object* v___x_1476_; 
v___x_1476_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1360_ = v___x_1476_;
goto v___jp_1359_;
}
}
case 12:
{
lean_object* v___x_1477_; uint8_t v___x_1478_; 
v___x_1477_ = lean_unsigned_to_nat(1024u);
v___x_1478_ = lean_nat_dec_le(v___x_1477_, v_prec_1106_);
if (v___x_1478_ == 0)
{
lean_object* v___x_1479_; 
v___x_1479_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1367_ = v___x_1479_;
goto v___jp_1366_;
}
else
{
lean_object* v___x_1480_; 
v___x_1480_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1367_ = v___x_1480_;
goto v___jp_1366_;
}
}
case 13:
{
lean_object* v___x_1481_; uint8_t v___x_1482_; 
v___x_1481_ = lean_unsigned_to_nat(1024u);
v___x_1482_ = lean_nat_dec_le(v___x_1481_, v_prec_1106_);
if (v___x_1482_ == 0)
{
lean_object* v___x_1483_; 
v___x_1483_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1374_ = v___x_1483_;
goto v___jp_1373_;
}
else
{
lean_object* v___x_1484_; 
v___x_1484_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1374_ = v___x_1484_;
goto v___jp_1373_;
}
}
case 14:
{
lean_object* v___x_1485_; uint8_t v___x_1486_; 
v___x_1485_ = lean_unsigned_to_nat(1024u);
v___x_1486_ = lean_nat_dec_le(v___x_1485_, v_prec_1106_);
if (v___x_1486_ == 0)
{
lean_object* v___x_1487_; 
v___x_1487_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1381_ = v___x_1487_;
goto v___jp_1380_;
}
else
{
lean_object* v___x_1488_; 
v___x_1488_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1381_ = v___x_1488_;
goto v___jp_1380_;
}
}
case 15:
{
lean_object* v___x_1489_; uint8_t v___x_1490_; 
v___x_1489_ = lean_unsigned_to_nat(1024u);
v___x_1490_ = lean_nat_dec_le(v___x_1489_, v_prec_1106_);
if (v___x_1490_ == 0)
{
lean_object* v___x_1491_; 
v___x_1491_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1388_ = v___x_1491_;
goto v___jp_1387_;
}
else
{
lean_object* v___x_1492_; 
v___x_1492_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1388_ = v___x_1492_;
goto v___jp_1387_;
}
}
case 16:
{
lean_object* v___x_1493_; uint8_t v___x_1494_; 
v___x_1493_ = lean_unsigned_to_nat(1024u);
v___x_1494_ = lean_nat_dec_le(v___x_1493_, v_prec_1106_);
if (v___x_1494_ == 0)
{
lean_object* v___x_1495_; 
v___x_1495_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1395_ = v___x_1495_;
goto v___jp_1394_;
}
else
{
lean_object* v___x_1496_; 
v___x_1496_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1395_ = v___x_1496_;
goto v___jp_1394_;
}
}
case 17:
{
lean_object* v___x_1497_; uint8_t v___x_1498_; 
v___x_1497_ = lean_unsigned_to_nat(1024u);
v___x_1498_ = lean_nat_dec_le(v___x_1497_, v_prec_1106_);
if (v___x_1498_ == 0)
{
lean_object* v___x_1499_; 
v___x_1499_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1402_ = v___x_1499_;
goto v___jp_1401_;
}
else
{
lean_object* v___x_1500_; 
v___x_1500_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1402_ = v___x_1500_;
goto v___jp_1401_;
}
}
case 18:
{
lean_object* v___x_1501_; uint8_t v___x_1502_; 
v___x_1501_ = lean_unsigned_to_nat(1024u);
v___x_1502_ = lean_nat_dec_le(v___x_1501_, v_prec_1106_);
if (v___x_1502_ == 0)
{
lean_object* v___x_1503_; 
v___x_1503_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1409_ = v___x_1503_;
goto v___jp_1408_;
}
else
{
lean_object* v___x_1504_; 
v___x_1504_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1409_ = v___x_1504_;
goto v___jp_1408_;
}
}
case 19:
{
lean_object* v___x_1505_; uint8_t v___x_1506_; 
v___x_1505_ = lean_unsigned_to_nat(1024u);
v___x_1506_ = lean_nat_dec_le(v___x_1505_, v_prec_1106_);
if (v___x_1506_ == 0)
{
lean_object* v___x_1507_; 
v___x_1507_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1416_ = v___x_1507_;
goto v___jp_1415_;
}
else
{
lean_object* v___x_1508_; 
v___x_1508_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1416_ = v___x_1508_;
goto v___jp_1415_;
}
}
case 20:
{
lean_object* v___x_1509_; uint8_t v___x_1510_; 
v___x_1509_ = lean_unsigned_to_nat(1024u);
v___x_1510_ = lean_nat_dec_le(v___x_1509_, v_prec_1106_);
if (v___x_1510_ == 0)
{
lean_object* v___x_1511_; 
v___x_1511_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1423_ = v___x_1511_;
goto v___jp_1422_;
}
else
{
lean_object* v___x_1512_; 
v___x_1512_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1423_ = v___x_1512_;
goto v___jp_1422_;
}
}
case 21:
{
lean_object* v_w_1513_; lean_object* v___x_1515_; uint8_t v_isShared_1516_; uint8_t v_isSharedCheck_1533_; 
v_w_1513_ = lean_ctor_get(v_x_1105_, 0);
v_isSharedCheck_1533_ = !lean_is_exclusive(v_x_1105_);
if (v_isSharedCheck_1533_ == 0)
{
v___x_1515_ = v_x_1105_;
v_isShared_1516_ = v_isSharedCheck_1533_;
goto v_resetjp_1514_;
}
else
{
lean_inc(v_w_1513_);
lean_dec(v_x_1105_);
v___x_1515_ = lean_box(0);
v_isShared_1516_ = v_isSharedCheck_1533_;
goto v_resetjp_1514_;
}
v_resetjp_1514_:
{
lean_object* v___y_1518_; lean_object* v___x_1529_; uint8_t v___x_1530_; 
v___x_1529_ = lean_unsigned_to_nat(1024u);
v___x_1530_ = lean_nat_dec_le(v___x_1529_, v_prec_1106_);
if (v___x_1530_ == 0)
{
lean_object* v___x_1531_; 
v___x_1531_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1518_ = v___x_1531_;
goto v___jp_1517_;
}
else
{
lean_object* v___x_1532_; 
v___x_1532_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1518_ = v___x_1532_;
goto v___jp_1517_;
}
v___jp_1517_:
{
lean_object* v___x_1519_; lean_object* v___x_1520_; lean_object* v___x_1522_; 
v___x_1519_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__94));
v___x_1520_ = l_Nat_reprFast(v_w_1513_);
if (v_isShared_1516_ == 0)
{
lean_ctor_set_tag(v___x_1515_, 3);
lean_ctor_set(v___x_1515_, 0, v___x_1520_);
v___x_1522_ = v___x_1515_;
goto v_reusejp_1521_;
}
else
{
lean_object* v_reuseFailAlloc_1528_; 
v_reuseFailAlloc_1528_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v_reuseFailAlloc_1528_, 0, v___x_1520_);
v___x_1522_ = v_reuseFailAlloc_1528_;
goto v_reusejp_1521_;
}
v_reusejp_1521_:
{
lean_object* v___x_1523_; lean_object* v___x_1524_; uint8_t v___x_1525_; lean_object* v___x_1526_; lean_object* v___x_1527_; 
v___x_1523_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1523_, 0, v___x_1519_);
lean_ctor_set(v___x_1523_, 1, v___x_1522_);
lean_inc(v___y_1518_);
v___x_1524_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1524_, 0, v___y_1518_);
lean_ctor_set(v___x_1524_, 1, v___x_1523_);
v___x_1525_ = 0;
v___x_1526_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1526_, 0, v___x_1524_);
lean_ctor_set_uint8(v___x_1526_, sizeof(void*)*1, v___x_1525_);
v___x_1527_ = l_Repr_addAppParen(v___x_1526_, v_prec_1106_);
return v___x_1527_;
}
}
}
}
case 22:
{
lean_object* v___x_1534_; uint8_t v___x_1535_; 
v___x_1534_ = lean_unsigned_to_nat(1024u);
v___x_1535_ = lean_nat_dec_le(v___x_1534_, v_prec_1106_);
if (v___x_1535_ == 0)
{
lean_object* v___x_1536_; 
v___x_1536_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1276_ = v___x_1536_;
goto v___jp_1275_;
}
else
{
lean_object* v___x_1537_; 
v___x_1537_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1276_ = v___x_1537_;
goto v___jp_1275_;
}
}
case 23:
{
lean_object* v___x_1538_; uint8_t v___x_1539_; 
v___x_1538_ = lean_unsigned_to_nat(1024u);
v___x_1539_ = lean_nat_dec_le(v___x_1538_, v_prec_1106_);
if (v___x_1539_ == 0)
{
lean_object* v___x_1540_; 
v___x_1540_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1269_ = v___x_1540_;
goto v___jp_1268_;
}
else
{
lean_object* v___x_1541_; 
v___x_1541_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1269_ = v___x_1541_;
goto v___jp_1268_;
}
}
case 24:
{
lean_object* v___x_1542_; uint8_t v___x_1543_; 
v___x_1542_ = lean_unsigned_to_nat(1024u);
v___x_1543_ = lean_nat_dec_le(v___x_1542_, v_prec_1106_);
if (v___x_1543_ == 0)
{
lean_object* v___x_1544_; 
v___x_1544_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1262_ = v___x_1544_;
goto v___jp_1261_;
}
else
{
lean_object* v___x_1545_; 
v___x_1545_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1262_ = v___x_1545_;
goto v___jp_1261_;
}
}
case 25:
{
lean_object* v___x_1546_; uint8_t v___x_1547_; 
v___x_1546_ = lean_unsigned_to_nat(1024u);
v___x_1547_ = lean_nat_dec_le(v___x_1546_, v_prec_1106_);
if (v___x_1547_ == 0)
{
lean_object* v___x_1548_; 
v___x_1548_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1255_ = v___x_1548_;
goto v___jp_1254_;
}
else
{
lean_object* v___x_1549_; 
v___x_1549_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1255_ = v___x_1549_;
goto v___jp_1254_;
}
}
case 26:
{
lean_object* v___x_1550_; uint8_t v___x_1551_; 
v___x_1550_ = lean_unsigned_to_nat(1024u);
v___x_1551_ = lean_nat_dec_le(v___x_1550_, v_prec_1106_);
if (v___x_1551_ == 0)
{
lean_object* v___x_1552_; 
v___x_1552_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1248_ = v___x_1552_;
goto v___jp_1247_;
}
else
{
lean_object* v___x_1553_; 
v___x_1553_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1248_ = v___x_1553_;
goto v___jp_1247_;
}
}
case 27:
{
lean_object* v___x_1554_; uint8_t v___x_1555_; 
v___x_1554_ = lean_unsigned_to_nat(1024u);
v___x_1555_ = lean_nat_dec_le(v___x_1554_, v_prec_1106_);
if (v___x_1555_ == 0)
{
lean_object* v___x_1556_; 
v___x_1556_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1241_ = v___x_1556_;
goto v___jp_1240_;
}
else
{
lean_object* v___x_1557_; 
v___x_1557_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1241_ = v___x_1557_;
goto v___jp_1240_;
}
}
case 28:
{
lean_object* v___x_1558_; uint8_t v___x_1559_; 
v___x_1558_ = lean_unsigned_to_nat(1024u);
v___x_1559_ = lean_nat_dec_le(v___x_1558_, v_prec_1106_);
if (v___x_1559_ == 0)
{
lean_object* v___x_1560_; 
v___x_1560_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1234_ = v___x_1560_;
goto v___jp_1233_;
}
else
{
lean_object* v___x_1561_; 
v___x_1561_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1234_ = v___x_1561_;
goto v___jp_1233_;
}
}
case 29:
{
lean_object* v___x_1562_; uint8_t v___x_1563_; 
v___x_1562_ = lean_unsigned_to_nat(1024u);
v___x_1563_ = lean_nat_dec_le(v___x_1562_, v_prec_1106_);
if (v___x_1563_ == 0)
{
lean_object* v___x_1564_; 
v___x_1564_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1227_ = v___x_1564_;
goto v___jp_1226_;
}
else
{
lean_object* v___x_1565_; 
v___x_1565_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1227_ = v___x_1565_;
goto v___jp_1226_;
}
}
case 30:
{
lean_object* v___x_1566_; uint8_t v___x_1567_; 
v___x_1566_ = lean_unsigned_to_nat(1024u);
v___x_1567_ = lean_nat_dec_le(v___x_1566_, v_prec_1106_);
if (v___x_1567_ == 0)
{
lean_object* v___x_1568_; 
v___x_1568_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1220_ = v___x_1568_;
goto v___jp_1219_;
}
else
{
lean_object* v___x_1569_; 
v___x_1569_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1220_ = v___x_1569_;
goto v___jp_1219_;
}
}
case 31:
{
lean_object* v___x_1570_; uint8_t v___x_1571_; 
v___x_1570_ = lean_unsigned_to_nat(1024u);
v___x_1571_ = lean_nat_dec_le(v___x_1570_, v_prec_1106_);
if (v___x_1571_ == 0)
{
lean_object* v___x_1572_; 
v___x_1572_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1213_ = v___x_1572_;
goto v___jp_1212_;
}
else
{
lean_object* v___x_1573_; 
v___x_1573_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1213_ = v___x_1573_;
goto v___jp_1212_;
}
}
case 32:
{
lean_object* v___x_1574_; uint8_t v___x_1575_; 
v___x_1574_ = lean_unsigned_to_nat(1024u);
v___x_1575_ = lean_nat_dec_le(v___x_1574_, v_prec_1106_);
if (v___x_1575_ == 0)
{
lean_object* v___x_1576_; 
v___x_1576_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1206_ = v___x_1576_;
goto v___jp_1205_;
}
else
{
lean_object* v___x_1577_; 
v___x_1577_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1206_ = v___x_1577_;
goto v___jp_1205_;
}
}
case 33:
{
lean_object* v___x_1578_; uint8_t v___x_1579_; 
v___x_1578_ = lean_unsigned_to_nat(1024u);
v___x_1579_ = lean_nat_dec_le(v___x_1578_, v_prec_1106_);
if (v___x_1579_ == 0)
{
lean_object* v___x_1580_; 
v___x_1580_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1199_ = v___x_1580_;
goto v___jp_1198_;
}
else
{
lean_object* v___x_1581_; 
v___x_1581_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1199_ = v___x_1581_;
goto v___jp_1198_;
}
}
case 34:
{
lean_object* v___x_1582_; uint8_t v___x_1583_; 
v___x_1582_ = lean_unsigned_to_nat(1024u);
v___x_1583_ = lean_nat_dec_le(v___x_1582_, v_prec_1106_);
if (v___x_1583_ == 0)
{
lean_object* v___x_1584_; 
v___x_1584_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1192_ = v___x_1584_;
goto v___jp_1191_;
}
else
{
lean_object* v___x_1585_; 
v___x_1585_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1192_ = v___x_1585_;
goto v___jp_1191_;
}
}
case 35:
{
lean_object* v___x_1586_; uint8_t v___x_1587_; 
v___x_1586_ = lean_unsigned_to_nat(1024u);
v___x_1587_ = lean_nat_dec_le(v___x_1586_, v_prec_1106_);
if (v___x_1587_ == 0)
{
lean_object* v___x_1588_; 
v___x_1588_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1185_ = v___x_1588_;
goto v___jp_1184_;
}
else
{
lean_object* v___x_1589_; 
v___x_1589_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1185_ = v___x_1589_;
goto v___jp_1184_;
}
}
case 36:
{
lean_object* v___x_1590_; uint8_t v___x_1591_; 
v___x_1590_ = lean_unsigned_to_nat(1024u);
v___x_1591_ = lean_nat_dec_le(v___x_1590_, v_prec_1106_);
if (v___x_1591_ == 0)
{
lean_object* v___x_1592_; 
v___x_1592_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1178_ = v___x_1592_;
goto v___jp_1177_;
}
else
{
lean_object* v___x_1593_; 
v___x_1593_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1178_ = v___x_1593_;
goto v___jp_1177_;
}
}
case 37:
{
lean_object* v___x_1594_; uint8_t v___x_1595_; 
v___x_1594_ = lean_unsigned_to_nat(1024u);
v___x_1595_ = lean_nat_dec_le(v___x_1594_, v_prec_1106_);
if (v___x_1595_ == 0)
{
lean_object* v___x_1596_; 
v___x_1596_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1171_ = v___x_1596_;
goto v___jp_1170_;
}
else
{
lean_object* v___x_1597_; 
v___x_1597_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1171_ = v___x_1597_;
goto v___jp_1170_;
}
}
case 38:
{
lean_object* v___x_1598_; uint8_t v___x_1599_; 
v___x_1598_ = lean_unsigned_to_nat(1024u);
v___x_1599_ = lean_nat_dec_le(v___x_1598_, v_prec_1106_);
if (v___x_1599_ == 0)
{
lean_object* v___x_1600_; 
v___x_1600_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1164_ = v___x_1600_;
goto v___jp_1163_;
}
else
{
lean_object* v___x_1601_; 
v___x_1601_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1164_ = v___x_1601_;
goto v___jp_1163_;
}
}
case 39:
{
lean_object* v___x_1602_; uint8_t v___x_1603_; 
v___x_1602_ = lean_unsigned_to_nat(1024u);
v___x_1603_ = lean_nat_dec_le(v___x_1602_, v_prec_1106_);
if (v___x_1603_ == 0)
{
lean_object* v___x_1604_; 
v___x_1604_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1157_ = v___x_1604_;
goto v___jp_1156_;
}
else
{
lean_object* v___x_1605_; 
v___x_1605_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1157_ = v___x_1605_;
goto v___jp_1156_;
}
}
case 40:
{
lean_object* v___x_1606_; uint8_t v___x_1607_; 
v___x_1606_ = lean_unsigned_to_nat(1024u);
v___x_1607_ = lean_nat_dec_le(v___x_1606_, v_prec_1106_);
if (v___x_1607_ == 0)
{
lean_object* v___x_1608_; 
v___x_1608_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1150_ = v___x_1608_;
goto v___jp_1149_;
}
else
{
lean_object* v___x_1609_; 
v___x_1609_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1150_ = v___x_1609_;
goto v___jp_1149_;
}
}
case 41:
{
lean_object* v___x_1610_; uint8_t v___x_1611_; 
v___x_1610_ = lean_unsigned_to_nat(1024u);
v___x_1611_ = lean_nat_dec_le(v___x_1610_, v_prec_1106_);
if (v___x_1611_ == 0)
{
lean_object* v___x_1612_; 
v___x_1612_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1143_ = v___x_1612_;
goto v___jp_1142_;
}
else
{
lean_object* v___x_1613_; 
v___x_1613_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1143_ = v___x_1613_;
goto v___jp_1142_;
}
}
case 42:
{
lean_object* v___x_1614_; uint8_t v___x_1615_; 
v___x_1614_ = lean_unsigned_to_nat(1024u);
v___x_1615_ = lean_nat_dec_le(v___x_1614_, v_prec_1106_);
if (v___x_1615_ == 0)
{
lean_object* v___x_1616_; 
v___x_1616_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1136_ = v___x_1616_;
goto v___jp_1135_;
}
else
{
lean_object* v___x_1617_; 
v___x_1617_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1136_ = v___x_1617_;
goto v___jp_1135_;
}
}
case 43:
{
lean_object* v___x_1618_; uint8_t v___x_1619_; 
v___x_1618_ = lean_unsigned_to_nat(1024u);
v___x_1619_ = lean_nat_dec_le(v___x_1618_, v_prec_1106_);
if (v___x_1619_ == 0)
{
lean_object* v___x_1620_; 
v___x_1620_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1129_ = v___x_1620_;
goto v___jp_1128_;
}
else
{
lean_object* v___x_1621_; 
v___x_1621_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1129_ = v___x_1621_;
goto v___jp_1128_;
}
}
case 44:
{
lean_object* v___x_1622_; uint8_t v___x_1623_; 
v___x_1622_ = lean_unsigned_to_nat(1024u);
v___x_1623_ = lean_nat_dec_le(v___x_1622_, v_prec_1106_);
if (v___x_1623_ == 0)
{
lean_object* v___x_1624_; 
v___x_1624_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1122_ = v___x_1624_;
goto v___jp_1121_;
}
else
{
lean_object* v___x_1625_; 
v___x_1625_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1122_ = v___x_1625_;
goto v___jp_1121_;
}
}
case 45:
{
lean_object* v___x_1626_; uint8_t v___x_1627_; 
v___x_1626_ = lean_unsigned_to_nat(1024u);
v___x_1627_ = lean_nat_dec_le(v___x_1626_, v_prec_1106_);
if (v___x_1627_ == 0)
{
lean_object* v___x_1628_; 
v___x_1628_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1115_ = v___x_1628_;
goto v___jp_1114_;
}
else
{
lean_object* v___x_1629_; 
v___x_1629_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1115_ = v___x_1629_;
goto v___jp_1114_;
}
}
case 46:
{
lean_object* v___x_1630_; uint8_t v___x_1631_; 
v___x_1630_ = lean_unsigned_to_nat(1024u);
v___x_1631_ = lean_nat_dec_le(v___x_1630_, v_prec_1106_);
if (v___x_1631_ == 0)
{
lean_object* v___x_1632_; 
v___x_1632_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1108_ = v___x_1632_;
goto v___jp_1107_;
}
else
{
lean_object* v___x_1633_; 
v___x_1633_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1108_ = v___x_1633_;
goto v___jp_1107_;
}
}
case 47:
{
lean_object* v_hi_1634_; lean_object* v_lo_1635_; lean_object* v___x_1637_; uint8_t v_isShared_1638_; uint8_t v_isSharedCheck_1660_; 
v_hi_1634_ = lean_ctor_get(v_x_1105_, 0);
v_lo_1635_ = lean_ctor_get(v_x_1105_, 1);
v_isSharedCheck_1660_ = !lean_is_exclusive(v_x_1105_);
if (v_isSharedCheck_1660_ == 0)
{
v___x_1637_ = v_x_1105_;
v_isShared_1638_ = v_isSharedCheck_1660_;
goto v_resetjp_1636_;
}
else
{
lean_inc(v_lo_1635_);
lean_inc(v_hi_1634_);
lean_dec(v_x_1105_);
v___x_1637_ = lean_box(0);
v_isShared_1638_ = v_isSharedCheck_1660_;
goto v_resetjp_1636_;
}
v_resetjp_1636_:
{
lean_object* v___y_1640_; lean_object* v___x_1656_; uint8_t v___x_1657_; 
v___x_1656_ = lean_unsigned_to_nat(1024u);
v___x_1657_ = lean_nat_dec_le(v___x_1656_, v_prec_1106_);
if (v___x_1657_ == 0)
{
lean_object* v___x_1658_; 
v___x_1658_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1640_ = v___x_1658_;
goto v___jp_1639_;
}
else
{
lean_object* v___x_1659_; 
v___x_1659_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1640_ = v___x_1659_;
goto v___jp_1639_;
}
v___jp_1639_:
{
lean_object* v___x_1641_; lean_object* v___x_1642_; lean_object* v___x_1643_; lean_object* v___x_1644_; lean_object* v___x_1646_; 
v___x_1641_ = lean_box(1);
v___x_1642_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__97));
v___x_1643_ = l_Nat_reprFast(v_hi_1634_);
v___x_1644_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_1644_, 0, v___x_1643_);
if (v_isShared_1638_ == 0)
{
lean_ctor_set_tag(v___x_1637_, 5);
lean_ctor_set(v___x_1637_, 1, v___x_1644_);
lean_ctor_set(v___x_1637_, 0, v___x_1642_);
v___x_1646_ = v___x_1637_;
goto v_reusejp_1645_;
}
else
{
lean_object* v_reuseFailAlloc_1655_; 
v_reuseFailAlloc_1655_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_1655_, 0, v___x_1642_);
lean_ctor_set(v_reuseFailAlloc_1655_, 1, v___x_1644_);
v___x_1646_ = v_reuseFailAlloc_1655_;
goto v_reusejp_1645_;
}
v_reusejp_1645_:
{
lean_object* v___x_1647_; lean_object* v___x_1648_; lean_object* v___x_1649_; lean_object* v___x_1650_; lean_object* v___x_1651_; uint8_t v___x_1652_; lean_object* v___x_1653_; lean_object* v___x_1654_; 
v___x_1647_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1647_, 0, v___x_1646_);
lean_ctor_set(v___x_1647_, 1, v___x_1641_);
v___x_1648_ = l_Nat_reprFast(v_lo_1635_);
v___x_1649_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_1649_, 0, v___x_1648_);
v___x_1650_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1650_, 0, v___x_1647_);
lean_ctor_set(v___x_1650_, 1, v___x_1649_);
lean_inc(v___y_1640_);
v___x_1651_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1651_, 0, v___y_1640_);
lean_ctor_set(v___x_1651_, 1, v___x_1650_);
v___x_1652_ = 0;
v___x_1653_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1653_, 0, v___x_1651_);
lean_ctor_set_uint8(v___x_1653_, sizeof(void*)*1, v___x_1652_);
v___x_1654_ = l_Repr_addAppParen(v___x_1653_, v_prec_1106_);
return v___x_1654_;
}
}
}
}
case 48:
{
lean_object* v_n_1661_; lean_object* v___x_1663_; uint8_t v_isShared_1664_; uint8_t v_isSharedCheck_1681_; 
v_n_1661_ = lean_ctor_get(v_x_1105_, 0);
v_isSharedCheck_1681_ = !lean_is_exclusive(v_x_1105_);
if (v_isSharedCheck_1681_ == 0)
{
v___x_1663_ = v_x_1105_;
v_isShared_1664_ = v_isSharedCheck_1681_;
goto v_resetjp_1662_;
}
else
{
lean_inc(v_n_1661_);
lean_dec(v_x_1105_);
v___x_1663_ = lean_box(0);
v_isShared_1664_ = v_isSharedCheck_1681_;
goto v_resetjp_1662_;
}
v_resetjp_1662_:
{
lean_object* v___y_1666_; lean_object* v___x_1677_; uint8_t v___x_1678_; 
v___x_1677_ = lean_unsigned_to_nat(1024u);
v___x_1678_ = lean_nat_dec_le(v___x_1677_, v_prec_1106_);
if (v___x_1678_ == 0)
{
lean_object* v___x_1679_; 
v___x_1679_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1666_ = v___x_1679_;
goto v___jp_1665_;
}
else
{
lean_object* v___x_1680_; 
v___x_1680_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1666_ = v___x_1680_;
goto v___jp_1665_;
}
v___jp_1665_:
{
lean_object* v___x_1667_; lean_object* v___x_1668_; lean_object* v___x_1670_; 
v___x_1667_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__100));
v___x_1668_ = l_Nat_reprFast(v_n_1661_);
if (v_isShared_1664_ == 0)
{
lean_ctor_set_tag(v___x_1663_, 3);
lean_ctor_set(v___x_1663_, 0, v___x_1668_);
v___x_1670_ = v___x_1663_;
goto v_reusejp_1669_;
}
else
{
lean_object* v_reuseFailAlloc_1676_; 
v_reuseFailAlloc_1676_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v_reuseFailAlloc_1676_, 0, v___x_1668_);
v___x_1670_ = v_reuseFailAlloc_1676_;
goto v_reusejp_1669_;
}
v_reusejp_1669_:
{
lean_object* v___x_1671_; lean_object* v___x_1672_; uint8_t v___x_1673_; lean_object* v___x_1674_; lean_object* v___x_1675_; 
v___x_1671_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1671_, 0, v___x_1667_);
lean_ctor_set(v___x_1671_, 1, v___x_1670_);
lean_inc(v___y_1666_);
v___x_1672_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1672_, 0, v___y_1666_);
lean_ctor_set(v___x_1672_, 1, v___x_1671_);
v___x_1673_ = 0;
v___x_1674_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1674_, 0, v___x_1672_);
lean_ctor_set_uint8(v___x_1674_, sizeof(void*)*1, v___x_1673_);
v___x_1675_ = l_Repr_addAppParen(v___x_1674_, v_prec_1106_);
return v___x_1675_;
}
}
}
}
default: 
{
lean_object* v_n_1682_; lean_object* v___x_1684_; uint8_t v_isShared_1685_; uint8_t v_isSharedCheck_1702_; 
v_n_1682_ = lean_ctor_get(v_x_1105_, 0);
v_isSharedCheck_1702_ = !lean_is_exclusive(v_x_1105_);
if (v_isSharedCheck_1702_ == 0)
{
v___x_1684_ = v_x_1105_;
v_isShared_1685_ = v_isSharedCheck_1702_;
goto v_resetjp_1683_;
}
else
{
lean_inc(v_n_1682_);
lean_dec(v_x_1105_);
v___x_1684_ = lean_box(0);
v_isShared_1685_ = v_isSharedCheck_1702_;
goto v_resetjp_1683_;
}
v_resetjp_1683_:
{
lean_object* v___y_1687_; lean_object* v___x_1698_; uint8_t v___x_1699_; 
v___x_1698_ = lean_unsigned_to_nat(1024u);
v___x_1699_ = lean_nat_dec_le(v___x_1698_, v_prec_1106_);
if (v___x_1699_ == 0)
{
lean_object* v___x_1700_; 
v___x_1700_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_1687_ = v___x_1700_;
goto v___jp_1686_;
}
else
{
lean_object* v___x_1701_; 
v___x_1701_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_1687_ = v___x_1701_;
goto v___jp_1686_;
}
v___jp_1686_:
{
lean_object* v___x_1688_; lean_object* v___x_1689_; lean_object* v___x_1691_; 
v___x_1688_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__103));
v___x_1689_ = l_Nat_reprFast(v_n_1682_);
if (v_isShared_1685_ == 0)
{
lean_ctor_set_tag(v___x_1684_, 3);
lean_ctor_set(v___x_1684_, 0, v___x_1689_);
v___x_1691_ = v___x_1684_;
goto v_reusejp_1690_;
}
else
{
lean_object* v_reuseFailAlloc_1697_; 
v_reuseFailAlloc_1697_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v_reuseFailAlloc_1697_, 0, v___x_1689_);
v___x_1691_ = v_reuseFailAlloc_1697_;
goto v_reusejp_1690_;
}
v_reusejp_1690_:
{
lean_object* v___x_1692_; lean_object* v___x_1693_; uint8_t v___x_1694_; lean_object* v___x_1695_; lean_object* v___x_1696_; 
v___x_1692_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1692_, 0, v___x_1688_);
lean_ctor_set(v___x_1692_, 1, v___x_1691_);
lean_inc(v___y_1687_);
v___x_1693_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1693_, 0, v___y_1687_);
lean_ctor_set(v___x_1693_, 1, v___x_1692_);
v___x_1694_ = 0;
v___x_1695_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1695_, 0, v___x_1693_);
lean_ctor_set_uint8(v___x_1695_, sizeof(void*)*1, v___x_1694_);
v___x_1696_ = l_Repr_addAppParen(v___x_1695_, v_prec_1106_);
return v___x_1696_;
}
}
}
}
}
v___jp_1107_:
{
lean_object* v___x_1109_; lean_object* v___x_1110_; uint8_t v___x_1111_; lean_object* v___x_1112_; lean_object* v___x_1113_; 
v___x_1109_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__1));
lean_inc(v___y_1108_);
v___x_1110_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1110_, 0, v___y_1108_);
lean_ctor_set(v___x_1110_, 1, v___x_1109_);
v___x_1111_ = 0;
v___x_1112_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1112_, 0, v___x_1110_);
lean_ctor_set_uint8(v___x_1112_, sizeof(void*)*1, v___x_1111_);
v___x_1113_ = l_Repr_addAppParen(v___x_1112_, v_prec_1106_);
return v___x_1113_;
}
v___jp_1114_:
{
lean_object* v___x_1116_; lean_object* v___x_1117_; uint8_t v___x_1118_; lean_object* v___x_1119_; lean_object* v___x_1120_; 
v___x_1116_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__3));
lean_inc(v___y_1115_);
v___x_1117_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1117_, 0, v___y_1115_);
lean_ctor_set(v___x_1117_, 1, v___x_1116_);
v___x_1118_ = 0;
v___x_1119_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1119_, 0, v___x_1117_);
lean_ctor_set_uint8(v___x_1119_, sizeof(void*)*1, v___x_1118_);
v___x_1120_ = l_Repr_addAppParen(v___x_1119_, v_prec_1106_);
return v___x_1120_;
}
v___jp_1121_:
{
lean_object* v___x_1123_; lean_object* v___x_1124_; uint8_t v___x_1125_; lean_object* v___x_1126_; lean_object* v___x_1127_; 
v___x_1123_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__5));
lean_inc(v___y_1122_);
v___x_1124_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1124_, 0, v___y_1122_);
lean_ctor_set(v___x_1124_, 1, v___x_1123_);
v___x_1125_ = 0;
v___x_1126_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1126_, 0, v___x_1124_);
lean_ctor_set_uint8(v___x_1126_, sizeof(void*)*1, v___x_1125_);
v___x_1127_ = l_Repr_addAppParen(v___x_1126_, v_prec_1106_);
return v___x_1127_;
}
v___jp_1128_:
{
lean_object* v___x_1130_; lean_object* v___x_1131_; uint8_t v___x_1132_; lean_object* v___x_1133_; lean_object* v___x_1134_; 
v___x_1130_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__7));
lean_inc(v___y_1129_);
v___x_1131_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1131_, 0, v___y_1129_);
lean_ctor_set(v___x_1131_, 1, v___x_1130_);
v___x_1132_ = 0;
v___x_1133_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1133_, 0, v___x_1131_);
lean_ctor_set_uint8(v___x_1133_, sizeof(void*)*1, v___x_1132_);
v___x_1134_ = l_Repr_addAppParen(v___x_1133_, v_prec_1106_);
return v___x_1134_;
}
v___jp_1135_:
{
lean_object* v___x_1137_; lean_object* v___x_1138_; uint8_t v___x_1139_; lean_object* v___x_1140_; lean_object* v___x_1141_; 
v___x_1137_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__9));
lean_inc(v___y_1136_);
v___x_1138_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1138_, 0, v___y_1136_);
lean_ctor_set(v___x_1138_, 1, v___x_1137_);
v___x_1139_ = 0;
v___x_1140_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1140_, 0, v___x_1138_);
lean_ctor_set_uint8(v___x_1140_, sizeof(void*)*1, v___x_1139_);
v___x_1141_ = l_Repr_addAppParen(v___x_1140_, v_prec_1106_);
return v___x_1141_;
}
v___jp_1142_:
{
lean_object* v___x_1144_; lean_object* v___x_1145_; uint8_t v___x_1146_; lean_object* v___x_1147_; lean_object* v___x_1148_; 
v___x_1144_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__11));
lean_inc(v___y_1143_);
v___x_1145_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1145_, 0, v___y_1143_);
lean_ctor_set(v___x_1145_, 1, v___x_1144_);
v___x_1146_ = 0;
v___x_1147_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1147_, 0, v___x_1145_);
lean_ctor_set_uint8(v___x_1147_, sizeof(void*)*1, v___x_1146_);
v___x_1148_ = l_Repr_addAppParen(v___x_1147_, v_prec_1106_);
return v___x_1148_;
}
v___jp_1149_:
{
lean_object* v___x_1151_; lean_object* v___x_1152_; uint8_t v___x_1153_; lean_object* v___x_1154_; lean_object* v___x_1155_; 
v___x_1151_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__13));
lean_inc(v___y_1150_);
v___x_1152_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1152_, 0, v___y_1150_);
lean_ctor_set(v___x_1152_, 1, v___x_1151_);
v___x_1153_ = 0;
v___x_1154_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1154_, 0, v___x_1152_);
lean_ctor_set_uint8(v___x_1154_, sizeof(void*)*1, v___x_1153_);
v___x_1155_ = l_Repr_addAppParen(v___x_1154_, v_prec_1106_);
return v___x_1155_;
}
v___jp_1156_:
{
lean_object* v___x_1158_; lean_object* v___x_1159_; uint8_t v___x_1160_; lean_object* v___x_1161_; lean_object* v___x_1162_; 
v___x_1158_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__15));
lean_inc(v___y_1157_);
v___x_1159_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1159_, 0, v___y_1157_);
lean_ctor_set(v___x_1159_, 1, v___x_1158_);
v___x_1160_ = 0;
v___x_1161_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1161_, 0, v___x_1159_);
lean_ctor_set_uint8(v___x_1161_, sizeof(void*)*1, v___x_1160_);
v___x_1162_ = l_Repr_addAppParen(v___x_1161_, v_prec_1106_);
return v___x_1162_;
}
v___jp_1163_:
{
lean_object* v___x_1165_; lean_object* v___x_1166_; uint8_t v___x_1167_; lean_object* v___x_1168_; lean_object* v___x_1169_; 
v___x_1165_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__17));
lean_inc(v___y_1164_);
v___x_1166_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1166_, 0, v___y_1164_);
lean_ctor_set(v___x_1166_, 1, v___x_1165_);
v___x_1167_ = 0;
v___x_1168_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1168_, 0, v___x_1166_);
lean_ctor_set_uint8(v___x_1168_, sizeof(void*)*1, v___x_1167_);
v___x_1169_ = l_Repr_addAppParen(v___x_1168_, v_prec_1106_);
return v___x_1169_;
}
v___jp_1170_:
{
lean_object* v___x_1172_; lean_object* v___x_1173_; uint8_t v___x_1174_; lean_object* v___x_1175_; lean_object* v___x_1176_; 
v___x_1172_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__19));
lean_inc(v___y_1171_);
v___x_1173_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1173_, 0, v___y_1171_);
lean_ctor_set(v___x_1173_, 1, v___x_1172_);
v___x_1174_ = 0;
v___x_1175_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1175_, 0, v___x_1173_);
lean_ctor_set_uint8(v___x_1175_, sizeof(void*)*1, v___x_1174_);
v___x_1176_ = l_Repr_addAppParen(v___x_1175_, v_prec_1106_);
return v___x_1176_;
}
v___jp_1177_:
{
lean_object* v___x_1179_; lean_object* v___x_1180_; uint8_t v___x_1181_; lean_object* v___x_1182_; lean_object* v___x_1183_; 
v___x_1179_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__21));
lean_inc(v___y_1178_);
v___x_1180_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1180_, 0, v___y_1178_);
lean_ctor_set(v___x_1180_, 1, v___x_1179_);
v___x_1181_ = 0;
v___x_1182_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1182_, 0, v___x_1180_);
lean_ctor_set_uint8(v___x_1182_, sizeof(void*)*1, v___x_1181_);
v___x_1183_ = l_Repr_addAppParen(v___x_1182_, v_prec_1106_);
return v___x_1183_;
}
v___jp_1184_:
{
lean_object* v___x_1186_; lean_object* v___x_1187_; uint8_t v___x_1188_; lean_object* v___x_1189_; lean_object* v___x_1190_; 
v___x_1186_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__23));
lean_inc(v___y_1185_);
v___x_1187_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1187_, 0, v___y_1185_);
lean_ctor_set(v___x_1187_, 1, v___x_1186_);
v___x_1188_ = 0;
v___x_1189_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1189_, 0, v___x_1187_);
lean_ctor_set_uint8(v___x_1189_, sizeof(void*)*1, v___x_1188_);
v___x_1190_ = l_Repr_addAppParen(v___x_1189_, v_prec_1106_);
return v___x_1190_;
}
v___jp_1191_:
{
lean_object* v___x_1193_; lean_object* v___x_1194_; uint8_t v___x_1195_; lean_object* v___x_1196_; lean_object* v___x_1197_; 
v___x_1193_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__25));
lean_inc(v___y_1192_);
v___x_1194_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1194_, 0, v___y_1192_);
lean_ctor_set(v___x_1194_, 1, v___x_1193_);
v___x_1195_ = 0;
v___x_1196_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1196_, 0, v___x_1194_);
lean_ctor_set_uint8(v___x_1196_, sizeof(void*)*1, v___x_1195_);
v___x_1197_ = l_Repr_addAppParen(v___x_1196_, v_prec_1106_);
return v___x_1197_;
}
v___jp_1198_:
{
lean_object* v___x_1200_; lean_object* v___x_1201_; uint8_t v___x_1202_; lean_object* v___x_1203_; lean_object* v___x_1204_; 
v___x_1200_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__27));
lean_inc(v___y_1199_);
v___x_1201_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1201_, 0, v___y_1199_);
lean_ctor_set(v___x_1201_, 1, v___x_1200_);
v___x_1202_ = 0;
v___x_1203_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1203_, 0, v___x_1201_);
lean_ctor_set_uint8(v___x_1203_, sizeof(void*)*1, v___x_1202_);
v___x_1204_ = l_Repr_addAppParen(v___x_1203_, v_prec_1106_);
return v___x_1204_;
}
v___jp_1205_:
{
lean_object* v___x_1207_; lean_object* v___x_1208_; uint8_t v___x_1209_; lean_object* v___x_1210_; lean_object* v___x_1211_; 
v___x_1207_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__29));
lean_inc(v___y_1206_);
v___x_1208_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1208_, 0, v___y_1206_);
lean_ctor_set(v___x_1208_, 1, v___x_1207_);
v___x_1209_ = 0;
v___x_1210_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1210_, 0, v___x_1208_);
lean_ctor_set_uint8(v___x_1210_, sizeof(void*)*1, v___x_1209_);
v___x_1211_ = l_Repr_addAppParen(v___x_1210_, v_prec_1106_);
return v___x_1211_;
}
v___jp_1212_:
{
lean_object* v___x_1214_; lean_object* v___x_1215_; uint8_t v___x_1216_; lean_object* v___x_1217_; lean_object* v___x_1218_; 
v___x_1214_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__31));
lean_inc(v___y_1213_);
v___x_1215_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1215_, 0, v___y_1213_);
lean_ctor_set(v___x_1215_, 1, v___x_1214_);
v___x_1216_ = 0;
v___x_1217_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1217_, 0, v___x_1215_);
lean_ctor_set_uint8(v___x_1217_, sizeof(void*)*1, v___x_1216_);
v___x_1218_ = l_Repr_addAppParen(v___x_1217_, v_prec_1106_);
return v___x_1218_;
}
v___jp_1219_:
{
lean_object* v___x_1221_; lean_object* v___x_1222_; uint8_t v___x_1223_; lean_object* v___x_1224_; lean_object* v___x_1225_; 
v___x_1221_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__33));
lean_inc(v___y_1220_);
v___x_1222_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1222_, 0, v___y_1220_);
lean_ctor_set(v___x_1222_, 1, v___x_1221_);
v___x_1223_ = 0;
v___x_1224_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1224_, 0, v___x_1222_);
lean_ctor_set_uint8(v___x_1224_, sizeof(void*)*1, v___x_1223_);
v___x_1225_ = l_Repr_addAppParen(v___x_1224_, v_prec_1106_);
return v___x_1225_;
}
v___jp_1226_:
{
lean_object* v___x_1228_; lean_object* v___x_1229_; uint8_t v___x_1230_; lean_object* v___x_1231_; lean_object* v___x_1232_; 
v___x_1228_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__35));
lean_inc(v___y_1227_);
v___x_1229_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1229_, 0, v___y_1227_);
lean_ctor_set(v___x_1229_, 1, v___x_1228_);
v___x_1230_ = 0;
v___x_1231_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1231_, 0, v___x_1229_);
lean_ctor_set_uint8(v___x_1231_, sizeof(void*)*1, v___x_1230_);
v___x_1232_ = l_Repr_addAppParen(v___x_1231_, v_prec_1106_);
return v___x_1232_;
}
v___jp_1233_:
{
lean_object* v___x_1235_; lean_object* v___x_1236_; uint8_t v___x_1237_; lean_object* v___x_1238_; lean_object* v___x_1239_; 
v___x_1235_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__37));
lean_inc(v___y_1234_);
v___x_1236_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1236_, 0, v___y_1234_);
lean_ctor_set(v___x_1236_, 1, v___x_1235_);
v___x_1237_ = 0;
v___x_1238_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1238_, 0, v___x_1236_);
lean_ctor_set_uint8(v___x_1238_, sizeof(void*)*1, v___x_1237_);
v___x_1239_ = l_Repr_addAppParen(v___x_1238_, v_prec_1106_);
return v___x_1239_;
}
v___jp_1240_:
{
lean_object* v___x_1242_; lean_object* v___x_1243_; uint8_t v___x_1244_; lean_object* v___x_1245_; lean_object* v___x_1246_; 
v___x_1242_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__39));
lean_inc(v___y_1241_);
v___x_1243_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1243_, 0, v___y_1241_);
lean_ctor_set(v___x_1243_, 1, v___x_1242_);
v___x_1244_ = 0;
v___x_1245_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1245_, 0, v___x_1243_);
lean_ctor_set_uint8(v___x_1245_, sizeof(void*)*1, v___x_1244_);
v___x_1246_ = l_Repr_addAppParen(v___x_1245_, v_prec_1106_);
return v___x_1246_;
}
v___jp_1247_:
{
lean_object* v___x_1249_; lean_object* v___x_1250_; uint8_t v___x_1251_; lean_object* v___x_1252_; lean_object* v___x_1253_; 
v___x_1249_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__41));
lean_inc(v___y_1248_);
v___x_1250_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1250_, 0, v___y_1248_);
lean_ctor_set(v___x_1250_, 1, v___x_1249_);
v___x_1251_ = 0;
v___x_1252_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1252_, 0, v___x_1250_);
lean_ctor_set_uint8(v___x_1252_, sizeof(void*)*1, v___x_1251_);
v___x_1253_ = l_Repr_addAppParen(v___x_1252_, v_prec_1106_);
return v___x_1253_;
}
v___jp_1254_:
{
lean_object* v___x_1256_; lean_object* v___x_1257_; uint8_t v___x_1258_; lean_object* v___x_1259_; lean_object* v___x_1260_; 
v___x_1256_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__43));
lean_inc(v___y_1255_);
v___x_1257_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1257_, 0, v___y_1255_);
lean_ctor_set(v___x_1257_, 1, v___x_1256_);
v___x_1258_ = 0;
v___x_1259_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1259_, 0, v___x_1257_);
lean_ctor_set_uint8(v___x_1259_, sizeof(void*)*1, v___x_1258_);
v___x_1260_ = l_Repr_addAppParen(v___x_1259_, v_prec_1106_);
return v___x_1260_;
}
v___jp_1261_:
{
lean_object* v___x_1263_; lean_object* v___x_1264_; uint8_t v___x_1265_; lean_object* v___x_1266_; lean_object* v___x_1267_; 
v___x_1263_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__45));
lean_inc(v___y_1262_);
v___x_1264_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1264_, 0, v___y_1262_);
lean_ctor_set(v___x_1264_, 1, v___x_1263_);
v___x_1265_ = 0;
v___x_1266_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1266_, 0, v___x_1264_);
lean_ctor_set_uint8(v___x_1266_, sizeof(void*)*1, v___x_1265_);
v___x_1267_ = l_Repr_addAppParen(v___x_1266_, v_prec_1106_);
return v___x_1267_;
}
v___jp_1268_:
{
lean_object* v___x_1270_; lean_object* v___x_1271_; uint8_t v___x_1272_; lean_object* v___x_1273_; lean_object* v___x_1274_; 
v___x_1270_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__47));
lean_inc(v___y_1269_);
v___x_1271_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1271_, 0, v___y_1269_);
lean_ctor_set(v___x_1271_, 1, v___x_1270_);
v___x_1272_ = 0;
v___x_1273_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1273_, 0, v___x_1271_);
lean_ctor_set_uint8(v___x_1273_, sizeof(void*)*1, v___x_1272_);
v___x_1274_ = l_Repr_addAppParen(v___x_1273_, v_prec_1106_);
return v___x_1274_;
}
v___jp_1275_:
{
lean_object* v___x_1277_; lean_object* v___x_1278_; uint8_t v___x_1279_; lean_object* v___x_1280_; lean_object* v___x_1281_; 
v___x_1277_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__49));
lean_inc(v___y_1276_);
v___x_1278_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1278_, 0, v___y_1276_);
lean_ctor_set(v___x_1278_, 1, v___x_1277_);
v___x_1279_ = 0;
v___x_1280_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1280_, 0, v___x_1278_);
lean_ctor_set_uint8(v___x_1280_, sizeof(void*)*1, v___x_1279_);
v___x_1281_ = l_Repr_addAppParen(v___x_1280_, v_prec_1106_);
return v___x_1281_;
}
v___jp_1282_:
{
lean_object* v___x_1284_; lean_object* v___x_1285_; uint8_t v___x_1286_; lean_object* v___x_1287_; lean_object* v___x_1288_; 
v___x_1284_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__51));
lean_inc(v___y_1283_);
v___x_1285_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1285_, 0, v___y_1283_);
lean_ctor_set(v___x_1285_, 1, v___x_1284_);
v___x_1286_ = 0;
v___x_1287_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1287_, 0, v___x_1285_);
lean_ctor_set_uint8(v___x_1287_, sizeof(void*)*1, v___x_1286_);
v___x_1288_ = l_Repr_addAppParen(v___x_1287_, v_prec_1106_);
return v___x_1288_;
}
v___jp_1289_:
{
lean_object* v___x_1291_; lean_object* v___x_1292_; uint8_t v___x_1293_; lean_object* v___x_1294_; lean_object* v___x_1295_; 
v___x_1291_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__53));
lean_inc(v___y_1290_);
v___x_1292_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1292_, 0, v___y_1290_);
lean_ctor_set(v___x_1292_, 1, v___x_1291_);
v___x_1293_ = 0;
v___x_1294_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1294_, 0, v___x_1292_);
lean_ctor_set_uint8(v___x_1294_, sizeof(void*)*1, v___x_1293_);
v___x_1295_ = l_Repr_addAppParen(v___x_1294_, v_prec_1106_);
return v___x_1295_;
}
v___jp_1296_:
{
lean_object* v___x_1298_; lean_object* v___x_1299_; uint8_t v___x_1300_; lean_object* v___x_1301_; lean_object* v___x_1302_; 
v___x_1298_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__55));
lean_inc(v___y_1297_);
v___x_1299_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1299_, 0, v___y_1297_);
lean_ctor_set(v___x_1299_, 1, v___x_1298_);
v___x_1300_ = 0;
v___x_1301_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1301_, 0, v___x_1299_);
lean_ctor_set_uint8(v___x_1301_, sizeof(void*)*1, v___x_1300_);
v___x_1302_ = l_Repr_addAppParen(v___x_1301_, v_prec_1106_);
return v___x_1302_;
}
v___jp_1303_:
{
lean_object* v___x_1305_; lean_object* v___x_1306_; uint8_t v___x_1307_; lean_object* v___x_1308_; lean_object* v___x_1309_; 
v___x_1305_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__57));
lean_inc(v___y_1304_);
v___x_1306_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1306_, 0, v___y_1304_);
lean_ctor_set(v___x_1306_, 1, v___x_1305_);
v___x_1307_ = 0;
v___x_1308_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1308_, 0, v___x_1306_);
lean_ctor_set_uint8(v___x_1308_, sizeof(void*)*1, v___x_1307_);
v___x_1309_ = l_Repr_addAppParen(v___x_1308_, v_prec_1106_);
return v___x_1309_;
}
v___jp_1310_:
{
lean_object* v___x_1312_; lean_object* v___x_1313_; uint8_t v___x_1314_; lean_object* v___x_1315_; lean_object* v___x_1316_; 
v___x_1312_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__59));
lean_inc(v___y_1311_);
v___x_1313_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1313_, 0, v___y_1311_);
lean_ctor_set(v___x_1313_, 1, v___x_1312_);
v___x_1314_ = 0;
v___x_1315_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1315_, 0, v___x_1313_);
lean_ctor_set_uint8(v___x_1315_, sizeof(void*)*1, v___x_1314_);
v___x_1316_ = l_Repr_addAppParen(v___x_1315_, v_prec_1106_);
return v___x_1316_;
}
v___jp_1317_:
{
lean_object* v___x_1319_; lean_object* v___x_1320_; uint8_t v___x_1321_; lean_object* v___x_1322_; lean_object* v___x_1323_; 
v___x_1319_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__61));
lean_inc(v___y_1318_);
v___x_1320_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1320_, 0, v___y_1318_);
lean_ctor_set(v___x_1320_, 1, v___x_1319_);
v___x_1321_ = 0;
v___x_1322_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1322_, 0, v___x_1320_);
lean_ctor_set_uint8(v___x_1322_, sizeof(void*)*1, v___x_1321_);
v___x_1323_ = l_Repr_addAppParen(v___x_1322_, v_prec_1106_);
return v___x_1323_;
}
v___jp_1324_:
{
lean_object* v___x_1326_; lean_object* v___x_1327_; uint8_t v___x_1328_; lean_object* v___x_1329_; lean_object* v___x_1330_; 
v___x_1326_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__63));
lean_inc(v___y_1325_);
v___x_1327_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1327_, 0, v___y_1325_);
lean_ctor_set(v___x_1327_, 1, v___x_1326_);
v___x_1328_ = 0;
v___x_1329_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1329_, 0, v___x_1327_);
lean_ctor_set_uint8(v___x_1329_, sizeof(void*)*1, v___x_1328_);
v___x_1330_ = l_Repr_addAppParen(v___x_1329_, v_prec_1106_);
return v___x_1330_;
}
v___jp_1331_:
{
lean_object* v___x_1333_; lean_object* v___x_1334_; uint8_t v___x_1335_; lean_object* v___x_1336_; lean_object* v___x_1337_; 
v___x_1333_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__65));
lean_inc(v___y_1332_);
v___x_1334_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1334_, 0, v___y_1332_);
lean_ctor_set(v___x_1334_, 1, v___x_1333_);
v___x_1335_ = 0;
v___x_1336_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1336_, 0, v___x_1334_);
lean_ctor_set_uint8(v___x_1336_, sizeof(void*)*1, v___x_1335_);
v___x_1337_ = l_Repr_addAppParen(v___x_1336_, v_prec_1106_);
return v___x_1337_;
}
v___jp_1338_:
{
lean_object* v___x_1340_; lean_object* v___x_1341_; uint8_t v___x_1342_; lean_object* v___x_1343_; lean_object* v___x_1344_; 
v___x_1340_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__67));
lean_inc(v___y_1339_);
v___x_1341_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1341_, 0, v___y_1339_);
lean_ctor_set(v___x_1341_, 1, v___x_1340_);
v___x_1342_ = 0;
v___x_1343_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1343_, 0, v___x_1341_);
lean_ctor_set_uint8(v___x_1343_, sizeof(void*)*1, v___x_1342_);
v___x_1344_ = l_Repr_addAppParen(v___x_1343_, v_prec_1106_);
return v___x_1344_;
}
v___jp_1345_:
{
lean_object* v___x_1347_; lean_object* v___x_1348_; uint8_t v___x_1349_; lean_object* v___x_1350_; lean_object* v___x_1351_; 
v___x_1347_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__69));
lean_inc(v___y_1346_);
v___x_1348_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1348_, 0, v___y_1346_);
lean_ctor_set(v___x_1348_, 1, v___x_1347_);
v___x_1349_ = 0;
v___x_1350_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1350_, 0, v___x_1348_);
lean_ctor_set_uint8(v___x_1350_, sizeof(void*)*1, v___x_1349_);
v___x_1351_ = l_Repr_addAppParen(v___x_1350_, v_prec_1106_);
return v___x_1351_;
}
v___jp_1352_:
{
lean_object* v___x_1354_; lean_object* v___x_1355_; uint8_t v___x_1356_; lean_object* v___x_1357_; lean_object* v___x_1358_; 
v___x_1354_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__71));
lean_inc(v___y_1353_);
v___x_1355_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1355_, 0, v___y_1353_);
lean_ctor_set(v___x_1355_, 1, v___x_1354_);
v___x_1356_ = 0;
v___x_1357_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1357_, 0, v___x_1355_);
lean_ctor_set_uint8(v___x_1357_, sizeof(void*)*1, v___x_1356_);
v___x_1358_ = l_Repr_addAppParen(v___x_1357_, v_prec_1106_);
return v___x_1358_;
}
v___jp_1359_:
{
lean_object* v___x_1361_; lean_object* v___x_1362_; uint8_t v___x_1363_; lean_object* v___x_1364_; lean_object* v___x_1365_; 
v___x_1361_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__73));
lean_inc(v___y_1360_);
v___x_1362_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1362_, 0, v___y_1360_);
lean_ctor_set(v___x_1362_, 1, v___x_1361_);
v___x_1363_ = 0;
v___x_1364_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1364_, 0, v___x_1362_);
lean_ctor_set_uint8(v___x_1364_, sizeof(void*)*1, v___x_1363_);
v___x_1365_ = l_Repr_addAppParen(v___x_1364_, v_prec_1106_);
return v___x_1365_;
}
v___jp_1366_:
{
lean_object* v___x_1368_; lean_object* v___x_1369_; uint8_t v___x_1370_; lean_object* v___x_1371_; lean_object* v___x_1372_; 
v___x_1368_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__75));
lean_inc(v___y_1367_);
v___x_1369_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1369_, 0, v___y_1367_);
lean_ctor_set(v___x_1369_, 1, v___x_1368_);
v___x_1370_ = 0;
v___x_1371_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1371_, 0, v___x_1369_);
lean_ctor_set_uint8(v___x_1371_, sizeof(void*)*1, v___x_1370_);
v___x_1372_ = l_Repr_addAppParen(v___x_1371_, v_prec_1106_);
return v___x_1372_;
}
v___jp_1373_:
{
lean_object* v___x_1375_; lean_object* v___x_1376_; uint8_t v___x_1377_; lean_object* v___x_1378_; lean_object* v___x_1379_; 
v___x_1375_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__77));
lean_inc(v___y_1374_);
v___x_1376_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1376_, 0, v___y_1374_);
lean_ctor_set(v___x_1376_, 1, v___x_1375_);
v___x_1377_ = 0;
v___x_1378_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1378_, 0, v___x_1376_);
lean_ctor_set_uint8(v___x_1378_, sizeof(void*)*1, v___x_1377_);
v___x_1379_ = l_Repr_addAppParen(v___x_1378_, v_prec_1106_);
return v___x_1379_;
}
v___jp_1380_:
{
lean_object* v___x_1382_; lean_object* v___x_1383_; uint8_t v___x_1384_; lean_object* v___x_1385_; lean_object* v___x_1386_; 
v___x_1382_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__79));
lean_inc(v___y_1381_);
v___x_1383_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1383_, 0, v___y_1381_);
lean_ctor_set(v___x_1383_, 1, v___x_1382_);
v___x_1384_ = 0;
v___x_1385_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1385_, 0, v___x_1383_);
lean_ctor_set_uint8(v___x_1385_, sizeof(void*)*1, v___x_1384_);
v___x_1386_ = l_Repr_addAppParen(v___x_1385_, v_prec_1106_);
return v___x_1386_;
}
v___jp_1387_:
{
lean_object* v___x_1389_; lean_object* v___x_1390_; uint8_t v___x_1391_; lean_object* v___x_1392_; lean_object* v___x_1393_; 
v___x_1389_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__81));
lean_inc(v___y_1388_);
v___x_1390_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1390_, 0, v___y_1388_);
lean_ctor_set(v___x_1390_, 1, v___x_1389_);
v___x_1391_ = 0;
v___x_1392_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1392_, 0, v___x_1390_);
lean_ctor_set_uint8(v___x_1392_, sizeof(void*)*1, v___x_1391_);
v___x_1393_ = l_Repr_addAppParen(v___x_1392_, v_prec_1106_);
return v___x_1393_;
}
v___jp_1394_:
{
lean_object* v___x_1396_; lean_object* v___x_1397_; uint8_t v___x_1398_; lean_object* v___x_1399_; lean_object* v___x_1400_; 
v___x_1396_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__83));
lean_inc(v___y_1395_);
v___x_1397_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1397_, 0, v___y_1395_);
lean_ctor_set(v___x_1397_, 1, v___x_1396_);
v___x_1398_ = 0;
v___x_1399_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1399_, 0, v___x_1397_);
lean_ctor_set_uint8(v___x_1399_, sizeof(void*)*1, v___x_1398_);
v___x_1400_ = l_Repr_addAppParen(v___x_1399_, v_prec_1106_);
return v___x_1400_;
}
v___jp_1401_:
{
lean_object* v___x_1403_; lean_object* v___x_1404_; uint8_t v___x_1405_; lean_object* v___x_1406_; lean_object* v___x_1407_; 
v___x_1403_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__85));
lean_inc(v___y_1402_);
v___x_1404_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1404_, 0, v___y_1402_);
lean_ctor_set(v___x_1404_, 1, v___x_1403_);
v___x_1405_ = 0;
v___x_1406_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1406_, 0, v___x_1404_);
lean_ctor_set_uint8(v___x_1406_, sizeof(void*)*1, v___x_1405_);
v___x_1407_ = l_Repr_addAppParen(v___x_1406_, v_prec_1106_);
return v___x_1407_;
}
v___jp_1408_:
{
lean_object* v___x_1410_; lean_object* v___x_1411_; uint8_t v___x_1412_; lean_object* v___x_1413_; lean_object* v___x_1414_; 
v___x_1410_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__87));
lean_inc(v___y_1409_);
v___x_1411_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1411_, 0, v___y_1409_);
lean_ctor_set(v___x_1411_, 1, v___x_1410_);
v___x_1412_ = 0;
v___x_1413_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1413_, 0, v___x_1411_);
lean_ctor_set_uint8(v___x_1413_, sizeof(void*)*1, v___x_1412_);
v___x_1414_ = l_Repr_addAppParen(v___x_1413_, v_prec_1106_);
return v___x_1414_;
}
v___jp_1415_:
{
lean_object* v___x_1417_; lean_object* v___x_1418_; uint8_t v___x_1419_; lean_object* v___x_1420_; lean_object* v___x_1421_; 
v___x_1417_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__89));
lean_inc(v___y_1416_);
v___x_1418_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1418_, 0, v___y_1416_);
lean_ctor_set(v___x_1418_, 1, v___x_1417_);
v___x_1419_ = 0;
v___x_1420_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1420_, 0, v___x_1418_);
lean_ctor_set_uint8(v___x_1420_, sizeof(void*)*1, v___x_1419_);
v___x_1421_ = l_Repr_addAppParen(v___x_1420_, v_prec_1106_);
return v___x_1421_;
}
v___jp_1422_:
{
lean_object* v___x_1424_; lean_object* v___x_1425_; uint8_t v___x_1426_; lean_object* v___x_1427_; lean_object* v___x_1428_; 
v___x_1424_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprOp_repr___closed__91));
lean_inc(v___y_1423_);
v___x_1425_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1425_, 0, v___y_1423_);
lean_ctor_set(v___x_1425_, 1, v___x_1424_);
v___x_1426_ = 0;
v___x_1427_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1427_, 0, v___x_1425_);
lean_ctor_set_uint8(v___x_1427_, sizeof(void*)*1, v___x_1426_);
v___x_1428_ = l_Repr_addAppParen(v___x_1427_, v_prec_1106_);
return v___x_1428_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprOp_repr___boxed(lean_object* v_x_1703_, lean_object* v_prec_1704_){
_start:
{
lean_object* v_res_1705_; 
v_res_1705_ = lp_solar__verify_SolarVerify_Smt_instReprOp_repr(v_x_1703_, v_prec_1704_);
lean_dec(v_prec_1704_);
return v_res_1705_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_instDecidableEqOp_decEq(lean_object* v_x_1708_, lean_object* v_x_1709_){
_start:
{
lean_object* v___x_1710_; lean_object* v___x_1711_; uint8_t v_decide_1712_; 
v___x_1710_ = lp_solar__verify_SolarVerify_Smt_Op_ctorIdx(v_x_1708_);
v___x_1711_ = lp_solar__verify_SolarVerify_Smt_Op_ctorIdx(v_x_1709_);
v_decide_1712_ = lean_nat_dec_eq(v___x_1710_, v___x_1711_);
lean_dec(v___x_1711_);
lean_dec(v___x_1710_);
if (v_decide_1712_ == 0)
{
return v_decide_1712_;
}
else
{
switch(lean_obj_tag(v_x_1708_))
{
case 21:
{
lean_object* v_w_1713_; lean_object* v_w_1714_; uint8_t v___x_1715_; 
v_w_1713_ = lean_ctor_get(v_x_1708_, 0);
v_w_1714_ = lean_ctor_get(v_x_1709_, 0);
v___x_1715_ = lean_nat_dec_eq(v_w_1713_, v_w_1714_);
return v___x_1715_;
}
case 47:
{
lean_object* v_hi_1716_; lean_object* v_lo_1717_; lean_object* v_hi_1718_; lean_object* v_lo_1719_; uint8_t v___x_1720_; 
v_hi_1716_ = lean_ctor_get(v_x_1708_, 0);
v_lo_1717_ = lean_ctor_get(v_x_1708_, 1);
v_hi_1718_ = lean_ctor_get(v_x_1709_, 0);
v_lo_1719_ = lean_ctor_get(v_x_1709_, 1);
v___x_1720_ = lean_nat_dec_eq(v_hi_1716_, v_hi_1718_);
if (v___x_1720_ == 0)
{
return v___x_1720_;
}
else
{
uint8_t v___x_1721_; 
v___x_1721_ = lean_nat_dec_eq(v_lo_1717_, v_lo_1719_);
return v___x_1721_;
}
}
case 48:
{
lean_object* v_n_1722_; lean_object* v_n_1723_; uint8_t v___x_1724_; 
v_n_1722_ = lean_ctor_get(v_x_1708_, 0);
v_n_1723_ = lean_ctor_get(v_x_1709_, 0);
v___x_1724_ = lean_nat_dec_eq(v_n_1722_, v_n_1723_);
return v___x_1724_;
}
case 49:
{
lean_object* v_n_1725_; lean_object* v_n_1726_; uint8_t v___x_1727_; 
v_n_1725_ = lean_ctor_get(v_x_1708_, 0);
v_n_1726_ = lean_ctor_get(v_x_1709_, 0);
v___x_1727_ = lean_nat_dec_eq(v_n_1725_, v_n_1726_);
return v___x_1727_;
}
default: 
{
return v_decide_1712_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instDecidableEqOp_decEq___boxed(lean_object* v_x_1728_, lean_object* v_x_1729_){
_start:
{
uint8_t v_res_1730_; lean_object* v_r_1731_; 
v_res_1730_ = lp_solar__verify_SolarVerify_Smt_instDecidableEqOp_decEq(v_x_1728_, v_x_1729_);
lean_dec(v_x_1729_);
lean_dec(v_x_1728_);
v_r_1731_ = lean_box(v_res_1730_);
return v_r_1731_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_instDecidableEqOp(lean_object* v_x_1732_, lean_object* v_x_1733_){
_start:
{
uint8_t v___x_1734_; 
v___x_1734_ = lp_solar__verify_SolarVerify_Smt_instDecidableEqOp_decEq(v_x_1732_, v_x_1733_);
return v___x_1734_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instDecidableEqOp___boxed(lean_object* v_x_1735_, lean_object* v_x_1736_){
_start:
{
uint8_t v_res_1737_; lean_object* v_r_1738_; 
v_res_1737_ = lp_solar__verify_SolarVerify_Smt_instDecidableEqOp(v_x_1735_, v_x_1736_);
lean_dec(v_x_1736_);
lean_dec(v_x_1735_);
v_r_1738_ = lean_box(v_res_1737_);
return v_r_1738_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_instInhabitedOp_default(void){
_start:
{
lean_object* v___x_1739_; 
v___x_1739_ = lean_box(0);
return v___x_1739_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_instInhabitedOp(void){
_start:
{
lean_object* v___x_1740_; 
v___x_1740_ = lean_box(0);
return v___x_1740_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_ctorIdx(lean_object* v_x_1741_){
_start:
{
switch(lean_obj_tag(v_x_1741_))
{
case 0:
{
lean_object* v___x_1742_; 
v___x_1742_ = lean_unsigned_to_nat(0u);
return v___x_1742_;
}
case 1:
{
lean_object* v___x_1743_; 
v___x_1743_ = lean_unsigned_to_nat(1u);
return v___x_1743_;
}
case 2:
{
lean_object* v___x_1744_; 
v___x_1744_ = lean_unsigned_to_nat(2u);
return v___x_1744_;
}
case 3:
{
lean_object* v___x_1745_; 
v___x_1745_ = lean_unsigned_to_nat(3u);
return v___x_1745_;
}
case 4:
{
lean_object* v___x_1746_; 
v___x_1746_ = lean_unsigned_to_nat(4u);
return v___x_1746_;
}
case 5:
{
lean_object* v___x_1747_; 
v___x_1747_ = lean_unsigned_to_nat(5u);
return v___x_1747_;
}
case 6:
{
lean_object* v___x_1748_; 
v___x_1748_ = lean_unsigned_to_nat(6u);
return v___x_1748_;
}
case 7:
{
lean_object* v___x_1749_; 
v___x_1749_ = lean_unsigned_to_nat(7u);
return v___x_1749_;
}
case 8:
{
lean_object* v___x_1750_; 
v___x_1750_ = lean_unsigned_to_nat(8u);
return v___x_1750_;
}
case 9:
{
lean_object* v___x_1751_; 
v___x_1751_ = lean_unsigned_to_nat(9u);
return v___x_1751_;
}
default: 
{
lean_object* v___x_1752_; 
v___x_1752_ = lean_unsigned_to_nat(10u);
return v___x_1752_;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_ctorIdx___boxed(lean_object* v_x_1753_){
_start:
{
lean_object* v_res_1754_; 
v_res_1754_ = lp_solar__verify_SolarVerify_Smt_Term_ctorIdx(v_x_1753_);
lean_dec_ref(v_x_1753_);
return v_res_1754_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(lean_object* v_t_1755_, lean_object* v_k_1756_){
_start:
{
switch(lean_obj_tag(v_t_1755_))
{
case 0:
{
uint8_t v_b_1757_; lean_object* v___x_1758_; lean_object* v___x_1759_; 
v_b_1757_ = lean_ctor_get_uint8(v_t_1755_, 0);
lean_dec_ref_known(v_t_1755_, 0);
v___x_1758_ = lean_box(v_b_1757_);
v___x_1759_ = lean_apply_1(v_k_1756_, v___x_1758_);
return v___x_1759_;
}
case 1:
{
lean_object* v_i_1760_; lean_object* v___x_1761_; 
v_i_1760_ = lean_ctor_get(v_t_1755_, 0);
lean_inc(v_i_1760_);
lean_dec_ref_known(v_t_1755_, 1);
v___x_1761_ = lean_apply_1(v_k_1756_, v_i_1760_);
return v___x_1761_;
}
case 2:
{
lean_object* v_w_1762_; lean_object* v_n_1763_; lean_object* v___x_1764_; 
v_w_1762_ = lean_ctor_get(v_t_1755_, 0);
lean_inc(v_w_1762_);
v_n_1763_ = lean_ctor_get(v_t_1755_, 1);
lean_inc(v_n_1763_);
lean_dec_ref_known(v_t_1755_, 2);
v___x_1764_ = lean_apply_2(v_k_1756_, v_w_1762_, v_n_1763_);
return v___x_1764_;
}
case 3:
{
lean_object* v_f_1765_; lean_object* v_args_1766_; lean_object* v___x_1767_; 
v_f_1765_ = lean_ctor_get(v_t_1755_, 0);
lean_inc(v_f_1765_);
v_args_1766_ = lean_ctor_get(v_t_1755_, 1);
lean_inc(v_args_1766_);
lean_dec_ref_known(v_t_1755_, 2);
v___x_1767_ = lean_apply_2(v_k_1756_, v_f_1765_, v_args_1766_);
return v___x_1767_;
}
case 4:
{
lean_object* v_i_1768_; lean_object* v___x_1769_; 
v_i_1768_ = lean_ctor_get(v_t_1755_, 0);
lean_inc(v_i_1768_);
lean_dec_ref_known(v_t_1755_, 1);
v___x_1769_ = lean_apply_1(v_k_1756_, v_i_1768_);
return v___x_1769_;
}
case 5:
{
lean_object* v_op_1770_; lean_object* v_args_1771_; lean_object* v___x_1772_; 
v_op_1770_ = lean_ctor_get(v_t_1755_, 0);
lean_inc(v_op_1770_);
v_args_1771_ = lean_ctor_get(v_t_1755_, 1);
lean_inc(v_args_1771_);
lean_dec_ref_known(v_t_1755_, 2);
v___x_1772_ = lean_apply_2(v_k_1756_, v_op_1770_, v_args_1771_);
return v___x_1772_;
}
case 7:
{
lean_object* v_fields_1773_; lean_object* v___x_1774_; 
v_fields_1773_ = lean_ctor_get(v_t_1755_, 0);
lean_inc(v_fields_1773_);
lean_dec_ref_known(v_t_1755_, 1);
v___x_1774_ = lean_apply_1(v_k_1756_, v_fields_1773_);
return v___x_1774_;
}
default: 
{
lean_object* v_idx_1775_; lean_object* v_v_1776_; lean_object* v___x_1777_; 
v_idx_1775_ = lean_ctor_get(v_t_1755_, 0);
lean_inc(v_idx_1775_);
v_v_1776_ = lean_ctor_get(v_t_1755_, 1);
lean_inc_ref(v_v_1776_);
lean_dec_ref(v_t_1755_);
v___x_1777_ = lean_apply_2(v_k_1756_, v_idx_1775_, v_v_1776_);
return v___x_1777_;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_ctorElim(lean_object* v_motive__1_1778_, lean_object* v_ctorIdx_1779_, lean_object* v_t_1780_, lean_object* v_h_1781_, lean_object* v_k_1782_){
_start:
{
lean_object* v___x_1783_; 
v___x_1783_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1780_, v_k_1782_);
return v___x_1783_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_ctorElim___boxed(lean_object* v_motive__1_1784_, lean_object* v_ctorIdx_1785_, lean_object* v_t_1786_, lean_object* v_h_1787_, lean_object* v_k_1788_){
_start:
{
lean_object* v_res_1789_; 
v_res_1789_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim(v_motive__1_1784_, v_ctorIdx_1785_, v_t_1786_, v_h_1787_, v_k_1788_);
lean_dec(v_ctorIdx_1785_);
return v_res_1789_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_bool_elim___redArg(lean_object* v_t_1790_, lean_object* v_bool_1791_){
_start:
{
lean_object* v___x_1792_; 
v___x_1792_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1790_, v_bool_1791_);
return v___x_1792_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_bool_elim(lean_object* v_motive__1_1793_, lean_object* v_t_1794_, lean_object* v_h_1795_, lean_object* v_bool_1796_){
_start:
{
lean_object* v___x_1797_; 
v___x_1797_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1794_, v_bool_1796_);
return v___x_1797_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_int_elim___redArg(lean_object* v_t_1798_, lean_object* v_int_1799_){
_start:
{
lean_object* v___x_1800_; 
v___x_1800_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1798_, v_int_1799_);
return v___x_1800_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_int_elim(lean_object* v_motive__1_1801_, lean_object* v_t_1802_, lean_object* v_h_1803_, lean_object* v_int_1804_){
_start:
{
lean_object* v___x_1805_; 
v___x_1805_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1802_, v_int_1804_);
return v___x_1805_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_bv_elim___redArg(lean_object* v_t_1806_, lean_object* v_bv_1807_){
_start:
{
lean_object* v___x_1808_; 
v___x_1808_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1806_, v_bv_1807_);
return v___x_1808_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_bv_elim(lean_object* v_motive__1_1809_, lean_object* v_t_1810_, lean_object* v_h_1811_, lean_object* v_bv_1812_){
_start:
{
lean_object* v___x_1813_; 
v___x_1813_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1810_, v_bv_1812_);
return v___x_1813_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_fn_elim___redArg(lean_object* v_t_1814_, lean_object* v_fn_1815_){
_start:
{
lean_object* v___x_1816_; 
v___x_1816_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1814_, v_fn_1815_);
return v___x_1816_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_fn_elim(lean_object* v_motive__1_1817_, lean_object* v_t_1818_, lean_object* v_h_1819_, lean_object* v_fn_1820_){
_start:
{
lean_object* v___x_1821_; 
v___x_1821_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1818_, v_fn_1820_);
return v___x_1821_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_var_elim___redArg(lean_object* v_t_1822_, lean_object* v_var_1823_){
_start:
{
lean_object* v___x_1824_; 
v___x_1824_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1822_, v_var_1823_);
return v___x_1824_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_var_elim(lean_object* v_motive__1_1825_, lean_object* v_t_1826_, lean_object* v_h_1827_, lean_object* v_var_1828_){
_start:
{
lean_object* v___x_1829_; 
v___x_1829_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1826_, v_var_1828_);
return v___x_1829_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_app_elim___redArg(lean_object* v_t_1830_, lean_object* v_app_1831_){
_start:
{
lean_object* v___x_1832_; 
v___x_1832_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1830_, v_app_1831_);
return v___x_1832_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_app_elim(lean_object* v_motive__1_1833_, lean_object* v_t_1834_, lean_object* v_h_1835_, lean_object* v_app_1836_){
_start:
{
lean_object* v___x_1837_; 
v___x_1837_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1834_, v_app_1836_);
return v___x_1837_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_constArray_elim___redArg(lean_object* v_t_1838_, lean_object* v_constArray_1839_){
_start:
{
lean_object* v___x_1840_; 
v___x_1840_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1838_, v_constArray_1839_);
return v___x_1840_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_constArray_elim(lean_object* v_motive__1_1841_, lean_object* v_t_1842_, lean_object* v_h_1843_, lean_object* v_constArray_1844_){
_start:
{
lean_object* v___x_1845_; 
v___x_1845_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1842_, v_constArray_1844_);
return v___x_1845_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_mk_elim___redArg(lean_object* v_t_1846_, lean_object* v_mk_1847_){
_start:
{
lean_object* v___x_1848_; 
v___x_1848_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1846_, v_mk_1847_);
return v___x_1848_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_mk_elim(lean_object* v_motive__1_1849_, lean_object* v_t_1850_, lean_object* v_h_1851_, lean_object* v_mk_1852_){
_start:
{
lean_object* v___x_1853_; 
v___x_1853_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1850_, v_mk_1852_);
return v___x_1853_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_proj_elim___redArg(lean_object* v_t_1854_, lean_object* v_proj_1855_){
_start:
{
lean_object* v___x_1856_; 
v___x_1856_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1854_, v_proj_1855_);
return v___x_1856_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_proj_elim(lean_object* v_motive__1_1857_, lean_object* v_t_1858_, lean_object* v_h_1859_, lean_object* v_proj_1860_){
_start:
{
lean_object* v___x_1861_; 
v___x_1861_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1858_, v_proj_1860_);
return v___x_1861_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_forall_elim___redArg(lean_object* v_t_1862_, lean_object* v_forall_1863_){
_start:
{
lean_object* v___x_1864_; 
v___x_1864_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1862_, v_forall_1863_);
return v___x_1864_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_forall_elim(lean_object* v_motive__1_1865_, lean_object* v_t_1866_, lean_object* v_h_1867_, lean_object* v_forall_1868_){
_start:
{
lean_object* v___x_1869_; 
v___x_1869_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1866_, v_forall_1868_);
return v___x_1869_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_exists_elim___redArg(lean_object* v_t_1870_, lean_object* v_exists_1871_){
_start:
{
lean_object* v___x_1872_; 
v___x_1872_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1870_, v_exists_1871_);
return v___x_1872_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_Term_exists_elim(lean_object* v_motive__1_1873_, lean_object* v_t_1874_, lean_object* v_h_1875_, lean_object* v_exists_1876_){
_start:
{
lean_object* v___x_1877_; 
v___x_1877_ = lp_solar__verify_SolarVerify_Smt_Term_ctorElim___redArg(v_t_1874_, v_exists_1876_);
return v___x_1877_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1___redArg(lean_object* v_a_1878_){
_start:
{
if (lean_obj_tag(v_a_1878_) == 0)
{
lean_object* v___x_1879_; 
v___x_1879_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__1));
return v___x_1879_;
}
else
{
lean_object* v___x_1880_; lean_object* v___x_1881_; lean_object* v___x_1882_; lean_object* v___x_1883_; lean_object* v___x_1884_; lean_object* v___x_1885_; lean_object* v___x_1886_; lean_object* v___x_1887_; uint8_t v___x_1888_; lean_object* v___x_1889_; 
v___x_1880_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__5));
v___x_1881_ = lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0_spec__0(v_a_1878_, v___x_1880_);
v___x_1882_ = lean_obj_once(&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8, &lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8_once, _init_lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8);
v___x_1883_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__9));
v___x_1884_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1884_, 0, v___x_1883_);
lean_ctor_set(v___x_1884_, 1, v___x_1881_);
v___x_1885_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__10));
v___x_1886_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1886_, 0, v___x_1884_);
lean_ctor_set(v___x_1886_, 1, v___x_1885_);
v___x_1887_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1887_, 0, v___x_1882_);
lean_ctor_set(v___x_1887_, 1, v___x_1886_);
v___x_1888_ = 0;
v___x_1889_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1889_, 0, v___x_1887_);
lean_ctor_set_uint8(v___x_1889_, sizeof(void*)*1, v___x_1888_);
return v___x_1889_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0_spec__1_spec__3(lean_object* v_x_1914_, lean_object* v_x_1915_, lean_object* v_x_1916_){
_start:
{
if (lean_obj_tag(v_x_1916_) == 0)
{
lean_dec(v_x_1914_);
return v_x_1915_;
}
else
{
lean_object* v_head_1917_; lean_object* v_tail_1918_; lean_object* v___x_1920_; uint8_t v_isShared_1921_; uint8_t v_isSharedCheck_1929_; 
v_head_1917_ = lean_ctor_get(v_x_1916_, 0);
v_tail_1918_ = lean_ctor_get(v_x_1916_, 1);
v_isSharedCheck_1929_ = !lean_is_exclusive(v_x_1916_);
if (v_isSharedCheck_1929_ == 0)
{
v___x_1920_ = v_x_1916_;
v_isShared_1921_ = v_isSharedCheck_1929_;
goto v_resetjp_1919_;
}
else
{
lean_inc(v_tail_1918_);
lean_inc(v_head_1917_);
lean_dec(v_x_1916_);
v___x_1920_ = lean_box(0);
v_isShared_1921_ = v_isSharedCheck_1929_;
goto v_resetjp_1919_;
}
v_resetjp_1919_:
{
lean_object* v___x_1923_; 
lean_inc(v_x_1914_);
if (v_isShared_1921_ == 0)
{
lean_ctor_set_tag(v___x_1920_, 5);
lean_ctor_set(v___x_1920_, 1, v_x_1914_);
lean_ctor_set(v___x_1920_, 0, v_x_1915_);
v___x_1923_ = v___x_1920_;
goto v_reusejp_1922_;
}
else
{
lean_object* v_reuseFailAlloc_1928_; 
v_reuseFailAlloc_1928_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_1928_, 0, v_x_1915_);
lean_ctor_set(v_reuseFailAlloc_1928_, 1, v_x_1914_);
v___x_1923_ = v_reuseFailAlloc_1928_;
goto v_reusejp_1922_;
}
v_reusejp_1922_:
{
lean_object* v___x_1924_; lean_object* v___x_1925_; lean_object* v___x_1926_; 
v___x_1924_ = lean_unsigned_to_nat(0u);
v___x_1925_ = lp_solar__verify_SolarVerify_Smt_instReprTerm_repr(v_head_1917_, v___x_1924_);
v___x_1926_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1926_, 0, v___x_1923_);
lean_ctor_set(v___x_1926_, 1, v___x_1925_);
v_x_1915_ = v___x_1926_;
v_x_1916_ = v_tail_1918_;
goto _start;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0_spec__1(lean_object* v_x_1930_, lean_object* v_x_1931_, lean_object* v_x_1932_){
_start:
{
if (lean_obj_tag(v_x_1932_) == 0)
{
lean_dec(v_x_1930_);
return v_x_1931_;
}
else
{
lean_object* v_head_1933_; lean_object* v_tail_1934_; lean_object* v___x_1936_; uint8_t v_isShared_1937_; uint8_t v_isSharedCheck_1945_; 
v_head_1933_ = lean_ctor_get(v_x_1932_, 0);
v_tail_1934_ = lean_ctor_get(v_x_1932_, 1);
v_isSharedCheck_1945_ = !lean_is_exclusive(v_x_1932_);
if (v_isSharedCheck_1945_ == 0)
{
v___x_1936_ = v_x_1932_;
v_isShared_1937_ = v_isSharedCheck_1945_;
goto v_resetjp_1935_;
}
else
{
lean_inc(v_tail_1934_);
lean_inc(v_head_1933_);
lean_dec(v_x_1932_);
v___x_1936_ = lean_box(0);
v_isShared_1937_ = v_isSharedCheck_1945_;
goto v_resetjp_1935_;
}
v_resetjp_1935_:
{
lean_object* v___x_1939_; 
lean_inc(v_x_1930_);
if (v_isShared_1937_ == 0)
{
lean_ctor_set_tag(v___x_1936_, 5);
lean_ctor_set(v___x_1936_, 1, v_x_1930_);
lean_ctor_set(v___x_1936_, 0, v_x_1931_);
v___x_1939_ = v___x_1936_;
goto v_reusejp_1938_;
}
else
{
lean_object* v_reuseFailAlloc_1944_; 
v_reuseFailAlloc_1944_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_1944_, 0, v_x_1931_);
lean_ctor_set(v_reuseFailAlloc_1944_, 1, v_x_1930_);
v___x_1939_ = v_reuseFailAlloc_1944_;
goto v_reusejp_1938_;
}
v_reusejp_1938_:
{
lean_object* v___x_1940_; lean_object* v___x_1941_; lean_object* v___x_1942_; lean_object* v___x_1943_; 
v___x_1940_ = lean_unsigned_to_nat(0u);
v___x_1941_ = lp_solar__verify_SolarVerify_Smt_instReprTerm_repr(v_head_1933_, v___x_1940_);
v___x_1942_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1942_, 0, v___x_1939_);
lean_ctor_set(v___x_1942_, 1, v___x_1941_);
v___x_1943_ = lp_solar__verify_List_foldl___at___00List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0_spec__1_spec__3(v_x_1930_, v___x_1942_, v_tail_1934_);
return v___x_1943_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0(lean_object* v_x_1946_, lean_object* v_x_1947_){
_start:
{
if (lean_obj_tag(v_x_1946_) == 0)
{
lean_object* v___x_1948_; 
lean_dec(v_x_1947_);
v___x_1948_ = lean_box(0);
return v___x_1948_;
}
else
{
lean_object* v_tail_1949_; 
v_tail_1949_ = lean_ctor_get(v_x_1946_, 1);
if (lean_obj_tag(v_tail_1949_) == 0)
{
lean_object* v_head_1950_; lean_object* v___x_1951_; 
lean_dec(v_x_1947_);
v_head_1950_ = lean_ctor_get(v_x_1946_, 0);
lean_inc(v_head_1950_);
lean_dec_ref_known(v_x_1946_, 2);
v___x_1951_ = lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0___lam__0(v_head_1950_);
return v___x_1951_;
}
else
{
lean_object* v_head_1952_; lean_object* v___x_1953_; lean_object* v___x_1954_; 
lean_inc(v_tail_1949_);
v_head_1952_ = lean_ctor_get(v_x_1946_, 0);
lean_inc(v_head_1952_);
lean_dec_ref_known(v_x_1946_, 2);
v___x_1953_ = lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0___lam__0(v_head_1952_);
v___x_1954_ = lp_solar__verify_List_foldl___at___00Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0_spec__1(v_x_1947_, v___x_1953_, v_tail_1949_);
return v___x_1954_;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0___redArg(lean_object* v_a_1955_){
_start:
{
if (lean_obj_tag(v_a_1955_) == 0)
{
lean_object* v___x_1956_; 
v___x_1956_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__1));
return v___x_1956_;
}
else
{
lean_object* v___x_1957_; lean_object* v___x_1958_; lean_object* v___x_1959_; lean_object* v___x_1960_; lean_object* v___x_1961_; lean_object* v___x_1962_; lean_object* v___x_1963_; lean_object* v___x_1964_; uint8_t v___x_1965_; lean_object* v___x_1966_; 
v___x_1957_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__5));
v___x_1958_ = lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0(v_a_1955_, v___x_1957_);
v___x_1959_ = lean_obj_once(&lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8, &lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8_once, _init_lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__8);
v___x_1960_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__9));
v___x_1961_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1961_, 0, v___x_1960_);
lean_ctor_set(v___x_1961_, 1, v___x_1958_);
v___x_1962_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__10));
v___x_1963_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_1963_, 0, v___x_1961_);
lean_ctor_set(v___x_1963_, 1, v___x_1962_);
v___x_1964_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_1964_, 0, v___x_1959_);
lean_ctor_set(v___x_1964_, 1, v___x_1963_);
v___x_1965_ = 0;
v___x_1966_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_1966_, 0, v___x_1964_);
lean_ctor_set_uint8(v___x_1966_, sizeof(void*)*1, v___x_1965_);
return v___x_1966_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr(lean_object* v_x_2009_, lean_object* v_prec_2010_){
_start:
{
lean_object* v___y_2012_; lean_object* v___y_2013_; lean_object* v___y_2014_; 
switch(lean_obj_tag(v_x_2009_))
{
case 0:
{
uint8_t v_b_2020_; lean_object* v___y_2022_; lean_object* v___x_2030_; uint8_t v___x_2031_; 
v_b_2020_ = lean_ctor_get_uint8(v_x_2009_, 0);
lean_dec_ref_known(v_x_2009_, 0);
v___x_2030_ = lean_unsigned_to_nat(1024u);
v___x_2031_ = lean_nat_dec_le(v___x_2030_, v_prec_2010_);
if (v___x_2031_ == 0)
{
lean_object* v___x_2032_; 
v___x_2032_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2022_ = v___x_2032_;
goto v___jp_2021_;
}
else
{
lean_object* v___x_2033_; 
v___x_2033_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2022_ = v___x_2033_;
goto v___jp_2021_;
}
v___jp_2021_:
{
lean_object* v___x_2023_; lean_object* v___x_2024_; lean_object* v___x_2025_; lean_object* v___x_2026_; uint8_t v___x_2027_; lean_object* v___x_2028_; lean_object* v___x_2029_; 
v___x_2023_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__2));
v___x_2024_ = l_Bool_repr___redArg(v_b_2020_);
v___x_2025_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2025_, 0, v___x_2023_);
lean_ctor_set(v___x_2025_, 1, v___x_2024_);
lean_inc(v___y_2022_);
v___x_2026_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2026_, 0, v___y_2022_);
lean_ctor_set(v___x_2026_, 1, v___x_2025_);
v___x_2027_ = 0;
v___x_2028_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2028_, 0, v___x_2026_);
lean_ctor_set_uint8(v___x_2028_, sizeof(void*)*1, v___x_2027_);
v___x_2029_ = l_Repr_addAppParen(v___x_2028_, v_prec_2010_);
return v___x_2029_;
}
}
case 1:
{
lean_object* v_i_2034_; lean_object* v___x_2036_; uint8_t v_isShared_2037_; uint8_t v_isSharedCheck_2057_; 
v_i_2034_ = lean_ctor_get(v_x_2009_, 0);
v_isSharedCheck_2057_ = !lean_is_exclusive(v_x_2009_);
if (v_isSharedCheck_2057_ == 0)
{
v___x_2036_ = v_x_2009_;
v_isShared_2037_ = v_isSharedCheck_2057_;
goto v_resetjp_2035_;
}
else
{
lean_inc(v_i_2034_);
lean_dec(v_x_2009_);
v___x_2036_ = lean_box(0);
v_isShared_2037_ = v_isSharedCheck_2057_;
goto v_resetjp_2035_;
}
v_resetjp_2035_:
{
lean_object* v___y_2039_; lean_object* v___x_2053_; uint8_t v___x_2054_; 
v___x_2053_ = lean_unsigned_to_nat(1024u);
v___x_2054_ = lean_nat_dec_le(v___x_2053_, v_prec_2010_);
if (v___x_2054_ == 0)
{
lean_object* v___x_2055_; 
v___x_2055_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2039_ = v___x_2055_;
goto v___jp_2038_;
}
else
{
lean_object* v___x_2056_; 
v___x_2056_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2039_ = v___x_2056_;
goto v___jp_2038_;
}
v___jp_2038_:
{
lean_object* v___x_2040_; lean_object* v___x_2041_; uint8_t v___x_2042_; 
v___x_2040_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__5));
v___x_2041_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0, &lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0_once, _init_lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0);
v___x_2042_ = lean_int_dec_lt(v_i_2034_, v___x_2041_);
if (v___x_2042_ == 0)
{
lean_object* v___x_2043_; lean_object* v___x_2045_; 
v___x_2043_ = l_Int_repr(v_i_2034_);
lean_dec(v_i_2034_);
if (v_isShared_2037_ == 0)
{
lean_ctor_set_tag(v___x_2036_, 3);
lean_ctor_set(v___x_2036_, 0, v___x_2043_);
v___x_2045_ = v___x_2036_;
goto v_reusejp_2044_;
}
else
{
lean_object* v_reuseFailAlloc_2046_; 
v_reuseFailAlloc_2046_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2046_, 0, v___x_2043_);
v___x_2045_ = v_reuseFailAlloc_2046_;
goto v_reusejp_2044_;
}
v_reusejp_2044_:
{
v___y_2012_ = v___y_2039_;
v___y_2013_ = v___x_2040_;
v___y_2014_ = v___x_2045_;
goto v___jp_2011_;
}
}
else
{
lean_object* v___x_2047_; lean_object* v___x_2048_; lean_object* v___x_2050_; 
v___x_2047_ = lean_unsigned_to_nat(1024u);
v___x_2048_ = l_Int_repr(v_i_2034_);
lean_dec(v_i_2034_);
if (v_isShared_2037_ == 0)
{
lean_ctor_set_tag(v___x_2036_, 3);
lean_ctor_set(v___x_2036_, 0, v___x_2048_);
v___x_2050_ = v___x_2036_;
goto v_reusejp_2049_;
}
else
{
lean_object* v_reuseFailAlloc_2052_; 
v_reuseFailAlloc_2052_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2052_, 0, v___x_2048_);
v___x_2050_ = v_reuseFailAlloc_2052_;
goto v_reusejp_2049_;
}
v_reusejp_2049_:
{
lean_object* v___x_2051_; 
v___x_2051_ = l_Repr_addAppParen(v___x_2050_, v___x_2047_);
v___y_2012_ = v___y_2039_;
v___y_2013_ = v___x_2040_;
v___y_2014_ = v___x_2051_;
goto v___jp_2011_;
}
}
}
}
}
case 2:
{
lean_object* v_w_2058_; lean_object* v_n_2059_; lean_object* v___x_2061_; uint8_t v_isShared_2062_; uint8_t v_isSharedCheck_2084_; 
v_w_2058_ = lean_ctor_get(v_x_2009_, 0);
v_n_2059_ = lean_ctor_get(v_x_2009_, 1);
v_isSharedCheck_2084_ = !lean_is_exclusive(v_x_2009_);
if (v_isSharedCheck_2084_ == 0)
{
v___x_2061_ = v_x_2009_;
v_isShared_2062_ = v_isSharedCheck_2084_;
goto v_resetjp_2060_;
}
else
{
lean_inc(v_n_2059_);
lean_inc(v_w_2058_);
lean_dec(v_x_2009_);
v___x_2061_ = lean_box(0);
v_isShared_2062_ = v_isSharedCheck_2084_;
goto v_resetjp_2060_;
}
v_resetjp_2060_:
{
lean_object* v___y_2064_; lean_object* v___x_2080_; uint8_t v___x_2081_; 
v___x_2080_ = lean_unsigned_to_nat(1024u);
v___x_2081_ = lean_nat_dec_le(v___x_2080_, v_prec_2010_);
if (v___x_2081_ == 0)
{
lean_object* v___x_2082_; 
v___x_2082_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2064_ = v___x_2082_;
goto v___jp_2063_;
}
else
{
lean_object* v___x_2083_; 
v___x_2083_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2064_ = v___x_2083_;
goto v___jp_2063_;
}
v___jp_2063_:
{
lean_object* v___x_2065_; lean_object* v___x_2066_; lean_object* v___x_2067_; lean_object* v___x_2068_; lean_object* v___x_2070_; 
v___x_2065_ = lean_box(1);
v___x_2066_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__8));
v___x_2067_ = l_Nat_reprFast(v_w_2058_);
v___x_2068_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_2068_, 0, v___x_2067_);
if (v_isShared_2062_ == 0)
{
lean_ctor_set_tag(v___x_2061_, 5);
lean_ctor_set(v___x_2061_, 1, v___x_2068_);
lean_ctor_set(v___x_2061_, 0, v___x_2066_);
v___x_2070_ = v___x_2061_;
goto v_reusejp_2069_;
}
else
{
lean_object* v_reuseFailAlloc_2079_; 
v_reuseFailAlloc_2079_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2079_, 0, v___x_2066_);
lean_ctor_set(v_reuseFailAlloc_2079_, 1, v___x_2068_);
v___x_2070_ = v_reuseFailAlloc_2079_;
goto v_reusejp_2069_;
}
v_reusejp_2069_:
{
lean_object* v___x_2071_; lean_object* v___x_2072_; lean_object* v___x_2073_; lean_object* v___x_2074_; lean_object* v___x_2075_; uint8_t v___x_2076_; lean_object* v___x_2077_; lean_object* v___x_2078_; 
v___x_2071_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2071_, 0, v___x_2070_);
lean_ctor_set(v___x_2071_, 1, v___x_2065_);
v___x_2072_ = l_Nat_reprFast(v_n_2059_);
v___x_2073_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_2073_, 0, v___x_2072_);
v___x_2074_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2074_, 0, v___x_2071_);
lean_ctor_set(v___x_2074_, 1, v___x_2073_);
lean_inc(v___y_2064_);
v___x_2075_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2075_, 0, v___y_2064_);
lean_ctor_set(v___x_2075_, 1, v___x_2074_);
v___x_2076_ = 0;
v___x_2077_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2077_, 0, v___x_2075_);
lean_ctor_set_uint8(v___x_2077_, sizeof(void*)*1, v___x_2076_);
v___x_2078_ = l_Repr_addAppParen(v___x_2077_, v_prec_2010_);
return v___x_2078_;
}
}
}
}
case 3:
{
lean_object* v_f_2085_; lean_object* v_args_2086_; lean_object* v___x_2088_; uint8_t v_isShared_2089_; uint8_t v_isSharedCheck_2110_; 
v_f_2085_ = lean_ctor_get(v_x_2009_, 0);
v_args_2086_ = lean_ctor_get(v_x_2009_, 1);
v_isSharedCheck_2110_ = !lean_is_exclusive(v_x_2009_);
if (v_isSharedCheck_2110_ == 0)
{
v___x_2088_ = v_x_2009_;
v_isShared_2089_ = v_isSharedCheck_2110_;
goto v_resetjp_2087_;
}
else
{
lean_inc(v_args_2086_);
lean_inc(v_f_2085_);
lean_dec(v_x_2009_);
v___x_2088_ = lean_box(0);
v_isShared_2089_ = v_isSharedCheck_2110_;
goto v_resetjp_2087_;
}
v_resetjp_2087_:
{
lean_object* v___y_2091_; lean_object* v___x_2106_; uint8_t v___x_2107_; 
v___x_2106_ = lean_unsigned_to_nat(1024u);
v___x_2107_ = lean_nat_dec_le(v___x_2106_, v_prec_2010_);
if (v___x_2107_ == 0)
{
lean_object* v___x_2108_; 
v___x_2108_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2091_ = v___x_2108_;
goto v___jp_2090_;
}
else
{
lean_object* v___x_2109_; 
v___x_2109_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2091_ = v___x_2109_;
goto v___jp_2090_;
}
v___jp_2090_:
{
lean_object* v___x_2092_; lean_object* v___x_2093_; lean_object* v___x_2094_; lean_object* v___x_2095_; lean_object* v___x_2097_; 
v___x_2092_ = lean_box(1);
v___x_2093_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__11));
v___x_2094_ = l_Nat_reprFast(v_f_2085_);
v___x_2095_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_2095_, 0, v___x_2094_);
if (v_isShared_2089_ == 0)
{
lean_ctor_set_tag(v___x_2088_, 5);
lean_ctor_set(v___x_2088_, 1, v___x_2095_);
lean_ctor_set(v___x_2088_, 0, v___x_2093_);
v___x_2097_ = v___x_2088_;
goto v_reusejp_2096_;
}
else
{
lean_object* v_reuseFailAlloc_2105_; 
v_reuseFailAlloc_2105_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2105_, 0, v___x_2093_);
lean_ctor_set(v_reuseFailAlloc_2105_, 1, v___x_2095_);
v___x_2097_ = v_reuseFailAlloc_2105_;
goto v_reusejp_2096_;
}
v_reusejp_2096_:
{
lean_object* v___x_2098_; lean_object* v___x_2099_; lean_object* v___x_2100_; lean_object* v___x_2101_; uint8_t v___x_2102_; lean_object* v___x_2103_; lean_object* v___x_2104_; 
v___x_2098_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2098_, 0, v___x_2097_);
lean_ctor_set(v___x_2098_, 1, v___x_2092_);
v___x_2099_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0___redArg(v_args_2086_);
v___x_2100_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2100_, 0, v___x_2098_);
lean_ctor_set(v___x_2100_, 1, v___x_2099_);
lean_inc(v___y_2091_);
v___x_2101_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2101_, 0, v___y_2091_);
lean_ctor_set(v___x_2101_, 1, v___x_2100_);
v___x_2102_ = 0;
v___x_2103_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2103_, 0, v___x_2101_);
lean_ctor_set_uint8(v___x_2103_, sizeof(void*)*1, v___x_2102_);
v___x_2104_ = l_Repr_addAppParen(v___x_2103_, v_prec_2010_);
return v___x_2104_;
}
}
}
}
case 4:
{
lean_object* v_i_2111_; lean_object* v___x_2113_; uint8_t v_isShared_2114_; uint8_t v_isSharedCheck_2131_; 
v_i_2111_ = lean_ctor_get(v_x_2009_, 0);
v_isSharedCheck_2131_ = !lean_is_exclusive(v_x_2009_);
if (v_isSharedCheck_2131_ == 0)
{
v___x_2113_ = v_x_2009_;
v_isShared_2114_ = v_isSharedCheck_2131_;
goto v_resetjp_2112_;
}
else
{
lean_inc(v_i_2111_);
lean_dec(v_x_2009_);
v___x_2113_ = lean_box(0);
v_isShared_2114_ = v_isSharedCheck_2131_;
goto v_resetjp_2112_;
}
v_resetjp_2112_:
{
lean_object* v___y_2116_; lean_object* v___x_2127_; uint8_t v___x_2128_; 
v___x_2127_ = lean_unsigned_to_nat(1024u);
v___x_2128_ = lean_nat_dec_le(v___x_2127_, v_prec_2010_);
if (v___x_2128_ == 0)
{
lean_object* v___x_2129_; 
v___x_2129_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2116_ = v___x_2129_;
goto v___jp_2115_;
}
else
{
lean_object* v___x_2130_; 
v___x_2130_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2116_ = v___x_2130_;
goto v___jp_2115_;
}
v___jp_2115_:
{
lean_object* v___x_2117_; lean_object* v___x_2118_; lean_object* v___x_2120_; 
v___x_2117_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__14));
v___x_2118_ = l_Nat_reprFast(v_i_2111_);
if (v_isShared_2114_ == 0)
{
lean_ctor_set_tag(v___x_2113_, 3);
lean_ctor_set(v___x_2113_, 0, v___x_2118_);
v___x_2120_ = v___x_2113_;
goto v_reusejp_2119_;
}
else
{
lean_object* v_reuseFailAlloc_2126_; 
v_reuseFailAlloc_2126_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2126_, 0, v___x_2118_);
v___x_2120_ = v_reuseFailAlloc_2126_;
goto v_reusejp_2119_;
}
v_reusejp_2119_:
{
lean_object* v___x_2121_; lean_object* v___x_2122_; uint8_t v___x_2123_; lean_object* v___x_2124_; lean_object* v___x_2125_; 
v___x_2121_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2121_, 0, v___x_2117_);
lean_ctor_set(v___x_2121_, 1, v___x_2120_);
lean_inc(v___y_2116_);
v___x_2122_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2122_, 0, v___y_2116_);
lean_ctor_set(v___x_2122_, 1, v___x_2121_);
v___x_2123_ = 0;
v___x_2124_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2124_, 0, v___x_2122_);
lean_ctor_set_uint8(v___x_2124_, sizeof(void*)*1, v___x_2123_);
v___x_2125_ = l_Repr_addAppParen(v___x_2124_, v_prec_2010_);
return v___x_2125_;
}
}
}
}
case 5:
{
lean_object* v_op_2132_; lean_object* v_args_2133_; lean_object* v___x_2135_; uint8_t v_isShared_2136_; uint8_t v_isSharedCheck_2157_; 
v_op_2132_ = lean_ctor_get(v_x_2009_, 0);
v_args_2133_ = lean_ctor_get(v_x_2009_, 1);
v_isSharedCheck_2157_ = !lean_is_exclusive(v_x_2009_);
if (v_isSharedCheck_2157_ == 0)
{
v___x_2135_ = v_x_2009_;
v_isShared_2136_ = v_isSharedCheck_2157_;
goto v_resetjp_2134_;
}
else
{
lean_inc(v_args_2133_);
lean_inc(v_op_2132_);
lean_dec(v_x_2009_);
v___x_2135_ = lean_box(0);
v_isShared_2136_ = v_isSharedCheck_2157_;
goto v_resetjp_2134_;
}
v_resetjp_2134_:
{
lean_object* v___y_2138_; lean_object* v___x_2153_; uint8_t v___x_2154_; 
v___x_2153_ = lean_unsigned_to_nat(1024u);
v___x_2154_ = lean_nat_dec_le(v___x_2153_, v_prec_2010_);
if (v___x_2154_ == 0)
{
lean_object* v___x_2155_; 
v___x_2155_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2138_ = v___x_2155_;
goto v___jp_2137_;
}
else
{
lean_object* v___x_2156_; 
v___x_2156_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2138_ = v___x_2156_;
goto v___jp_2137_;
}
v___jp_2137_:
{
lean_object* v___x_2139_; lean_object* v___x_2140_; lean_object* v___x_2141_; lean_object* v___x_2142_; lean_object* v___x_2144_; 
v___x_2139_ = lean_box(1);
v___x_2140_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__17));
v___x_2141_ = lean_unsigned_to_nat(1024u);
v___x_2142_ = lp_solar__verify_SolarVerify_Smt_instReprOp_repr(v_op_2132_, v___x_2141_);
if (v_isShared_2136_ == 0)
{
lean_ctor_set(v___x_2135_, 1, v___x_2142_);
lean_ctor_set(v___x_2135_, 0, v___x_2140_);
v___x_2144_ = v___x_2135_;
goto v_reusejp_2143_;
}
else
{
lean_object* v_reuseFailAlloc_2152_; 
v_reuseFailAlloc_2152_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2152_, 0, v___x_2140_);
lean_ctor_set(v_reuseFailAlloc_2152_, 1, v___x_2142_);
v___x_2144_ = v_reuseFailAlloc_2152_;
goto v_reusejp_2143_;
}
v_reusejp_2143_:
{
lean_object* v___x_2145_; lean_object* v___x_2146_; lean_object* v___x_2147_; lean_object* v___x_2148_; uint8_t v___x_2149_; lean_object* v___x_2150_; lean_object* v___x_2151_; 
v___x_2145_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2145_, 0, v___x_2144_);
lean_ctor_set(v___x_2145_, 1, v___x_2139_);
v___x_2146_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0___redArg(v_args_2133_);
v___x_2147_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2147_, 0, v___x_2145_);
lean_ctor_set(v___x_2147_, 1, v___x_2146_);
lean_inc(v___y_2138_);
v___x_2148_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2148_, 0, v___y_2138_);
lean_ctor_set(v___x_2148_, 1, v___x_2147_);
v___x_2149_ = 0;
v___x_2150_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2150_, 0, v___x_2148_);
lean_ctor_set_uint8(v___x_2150_, sizeof(void*)*1, v___x_2149_);
v___x_2151_ = l_Repr_addAppParen(v___x_2150_, v_prec_2010_);
return v___x_2151_;
}
}
}
}
case 6:
{
lean_object* v_idx_2158_; lean_object* v_v_2159_; lean_object* v___x_2161_; uint8_t v_isShared_2162_; uint8_t v_isSharedCheck_2182_; 
v_idx_2158_ = lean_ctor_get(v_x_2009_, 0);
v_v_2159_ = lean_ctor_get(v_x_2009_, 1);
v_isSharedCheck_2182_ = !lean_is_exclusive(v_x_2009_);
if (v_isSharedCheck_2182_ == 0)
{
v___x_2161_ = v_x_2009_;
v_isShared_2162_ = v_isSharedCheck_2182_;
goto v_resetjp_2160_;
}
else
{
lean_inc(v_v_2159_);
lean_inc(v_idx_2158_);
lean_dec(v_x_2009_);
v___x_2161_ = lean_box(0);
v_isShared_2162_ = v_isSharedCheck_2182_;
goto v_resetjp_2160_;
}
v_resetjp_2160_:
{
lean_object* v___x_2163_; lean_object* v___y_2165_; uint8_t v___x_2179_; 
v___x_2163_ = lean_unsigned_to_nat(1024u);
v___x_2179_ = lean_nat_dec_le(v___x_2163_, v_prec_2010_);
if (v___x_2179_ == 0)
{
lean_object* v___x_2180_; 
v___x_2180_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2165_ = v___x_2180_;
goto v___jp_2164_;
}
else
{
lean_object* v___x_2181_; 
v___x_2181_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2165_ = v___x_2181_;
goto v___jp_2164_;
}
v___jp_2164_:
{
lean_object* v___x_2166_; lean_object* v___x_2167_; lean_object* v___x_2168_; lean_object* v___x_2170_; 
v___x_2166_ = lean_box(1);
v___x_2167_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__20));
v___x_2168_ = lp_solar__verify_SolarVerify_Smt_instReprSrt_repr(v_idx_2158_, v___x_2163_);
if (v_isShared_2162_ == 0)
{
lean_ctor_set_tag(v___x_2161_, 5);
lean_ctor_set(v___x_2161_, 1, v___x_2168_);
lean_ctor_set(v___x_2161_, 0, v___x_2167_);
v___x_2170_ = v___x_2161_;
goto v_reusejp_2169_;
}
else
{
lean_object* v_reuseFailAlloc_2178_; 
v_reuseFailAlloc_2178_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2178_, 0, v___x_2167_);
lean_ctor_set(v_reuseFailAlloc_2178_, 1, v___x_2168_);
v___x_2170_ = v_reuseFailAlloc_2178_;
goto v_reusejp_2169_;
}
v_reusejp_2169_:
{
lean_object* v___x_2171_; lean_object* v___x_2172_; lean_object* v___x_2173_; lean_object* v___x_2174_; uint8_t v___x_2175_; lean_object* v___x_2176_; lean_object* v___x_2177_; 
v___x_2171_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2171_, 0, v___x_2170_);
lean_ctor_set(v___x_2171_, 1, v___x_2166_);
v___x_2172_ = lp_solar__verify_SolarVerify_Smt_instReprTerm_repr(v_v_2159_, v___x_2163_);
v___x_2173_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2173_, 0, v___x_2171_);
lean_ctor_set(v___x_2173_, 1, v___x_2172_);
lean_inc(v___y_2165_);
v___x_2174_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2174_, 0, v___y_2165_);
lean_ctor_set(v___x_2174_, 1, v___x_2173_);
v___x_2175_ = 0;
v___x_2176_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2176_, 0, v___x_2174_);
lean_ctor_set_uint8(v___x_2176_, sizeof(void*)*1, v___x_2175_);
v___x_2177_ = l_Repr_addAppParen(v___x_2176_, v_prec_2010_);
return v___x_2177_;
}
}
}
}
case 7:
{
lean_object* v_fields_2183_; lean_object* v___y_2185_; lean_object* v___x_2193_; uint8_t v___x_2194_; 
v_fields_2183_ = lean_ctor_get(v_x_2009_, 0);
lean_inc(v_fields_2183_);
lean_dec_ref_known(v_x_2009_, 1);
v___x_2193_ = lean_unsigned_to_nat(1024u);
v___x_2194_ = lean_nat_dec_le(v___x_2193_, v_prec_2010_);
if (v___x_2194_ == 0)
{
lean_object* v___x_2195_; 
v___x_2195_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2185_ = v___x_2195_;
goto v___jp_2184_;
}
else
{
lean_object* v___x_2196_; 
v___x_2196_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2185_ = v___x_2196_;
goto v___jp_2184_;
}
v___jp_2184_:
{
lean_object* v___x_2186_; lean_object* v___x_2187_; lean_object* v___x_2188_; lean_object* v___x_2189_; uint8_t v___x_2190_; lean_object* v___x_2191_; lean_object* v___x_2192_; 
v___x_2186_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__23));
v___x_2187_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0___redArg(v_fields_2183_);
v___x_2188_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2188_, 0, v___x_2186_);
lean_ctor_set(v___x_2188_, 1, v___x_2187_);
lean_inc(v___y_2185_);
v___x_2189_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2189_, 0, v___y_2185_);
lean_ctor_set(v___x_2189_, 1, v___x_2188_);
v___x_2190_ = 0;
v___x_2191_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2191_, 0, v___x_2189_);
lean_ctor_set_uint8(v___x_2191_, sizeof(void*)*1, v___x_2190_);
v___x_2192_ = l_Repr_addAppParen(v___x_2191_, v_prec_2010_);
return v___x_2192_;
}
}
case 8:
{
lean_object* v_i_2197_; lean_object* v_t_2198_; lean_object* v___x_2200_; uint8_t v_isShared_2201_; uint8_t v_isSharedCheck_2222_; 
v_i_2197_ = lean_ctor_get(v_x_2009_, 0);
v_t_2198_ = lean_ctor_get(v_x_2009_, 1);
v_isSharedCheck_2222_ = !lean_is_exclusive(v_x_2009_);
if (v_isSharedCheck_2222_ == 0)
{
v___x_2200_ = v_x_2009_;
v_isShared_2201_ = v_isSharedCheck_2222_;
goto v_resetjp_2199_;
}
else
{
lean_inc(v_t_2198_);
lean_inc(v_i_2197_);
lean_dec(v_x_2009_);
v___x_2200_ = lean_box(0);
v_isShared_2201_ = v_isSharedCheck_2222_;
goto v_resetjp_2199_;
}
v_resetjp_2199_:
{
lean_object* v___x_2202_; lean_object* v___y_2204_; uint8_t v___x_2219_; 
v___x_2202_ = lean_unsigned_to_nat(1024u);
v___x_2219_ = lean_nat_dec_le(v___x_2202_, v_prec_2010_);
if (v___x_2219_ == 0)
{
lean_object* v___x_2220_; 
v___x_2220_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2204_ = v___x_2220_;
goto v___jp_2203_;
}
else
{
lean_object* v___x_2221_; 
v___x_2221_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2204_ = v___x_2221_;
goto v___jp_2203_;
}
v___jp_2203_:
{
lean_object* v___x_2205_; lean_object* v___x_2206_; lean_object* v___x_2207_; lean_object* v___x_2208_; lean_object* v___x_2210_; 
v___x_2205_ = lean_box(1);
v___x_2206_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__26));
v___x_2207_ = l_Nat_reprFast(v_i_2197_);
v___x_2208_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_2208_, 0, v___x_2207_);
if (v_isShared_2201_ == 0)
{
lean_ctor_set_tag(v___x_2200_, 5);
lean_ctor_set(v___x_2200_, 1, v___x_2208_);
lean_ctor_set(v___x_2200_, 0, v___x_2206_);
v___x_2210_ = v___x_2200_;
goto v_reusejp_2209_;
}
else
{
lean_object* v_reuseFailAlloc_2218_; 
v_reuseFailAlloc_2218_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2218_, 0, v___x_2206_);
lean_ctor_set(v_reuseFailAlloc_2218_, 1, v___x_2208_);
v___x_2210_ = v_reuseFailAlloc_2218_;
goto v_reusejp_2209_;
}
v_reusejp_2209_:
{
lean_object* v___x_2211_; lean_object* v___x_2212_; lean_object* v___x_2213_; lean_object* v___x_2214_; uint8_t v___x_2215_; lean_object* v___x_2216_; lean_object* v___x_2217_; 
v___x_2211_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2211_, 0, v___x_2210_);
lean_ctor_set(v___x_2211_, 1, v___x_2205_);
v___x_2212_ = lp_solar__verify_SolarVerify_Smt_instReprTerm_repr(v_t_2198_, v___x_2202_);
v___x_2213_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2213_, 0, v___x_2211_);
lean_ctor_set(v___x_2213_, 1, v___x_2212_);
lean_inc(v___y_2204_);
v___x_2214_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2214_, 0, v___y_2204_);
lean_ctor_set(v___x_2214_, 1, v___x_2213_);
v___x_2215_ = 0;
v___x_2216_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2216_, 0, v___x_2214_);
lean_ctor_set_uint8(v___x_2216_, sizeof(void*)*1, v___x_2215_);
v___x_2217_ = l_Repr_addAppParen(v___x_2216_, v_prec_2010_);
return v___x_2217_;
}
}
}
}
case 9:
{
lean_object* v_binders_2223_; lean_object* v_body_2224_; lean_object* v___x_2226_; uint8_t v_isShared_2227_; uint8_t v_isSharedCheck_2247_; 
v_binders_2223_ = lean_ctor_get(v_x_2009_, 0);
v_body_2224_ = lean_ctor_get(v_x_2009_, 1);
v_isSharedCheck_2247_ = !lean_is_exclusive(v_x_2009_);
if (v_isSharedCheck_2247_ == 0)
{
v___x_2226_ = v_x_2009_;
v_isShared_2227_ = v_isSharedCheck_2247_;
goto v_resetjp_2225_;
}
else
{
lean_inc(v_body_2224_);
lean_inc(v_binders_2223_);
lean_dec(v_x_2009_);
v___x_2226_ = lean_box(0);
v_isShared_2227_ = v_isSharedCheck_2247_;
goto v_resetjp_2225_;
}
v_resetjp_2225_:
{
lean_object* v___x_2228_; lean_object* v___y_2230_; uint8_t v___x_2244_; 
v___x_2228_ = lean_unsigned_to_nat(1024u);
v___x_2244_ = lean_nat_dec_le(v___x_2228_, v_prec_2010_);
if (v___x_2244_ == 0)
{
lean_object* v___x_2245_; 
v___x_2245_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2230_ = v___x_2245_;
goto v___jp_2229_;
}
else
{
lean_object* v___x_2246_; 
v___x_2246_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2230_ = v___x_2246_;
goto v___jp_2229_;
}
v___jp_2229_:
{
lean_object* v___x_2231_; lean_object* v___x_2232_; lean_object* v___x_2233_; lean_object* v___x_2235_; 
v___x_2231_ = lean_box(1);
v___x_2232_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__29));
v___x_2233_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1___redArg(v_binders_2223_);
if (v_isShared_2227_ == 0)
{
lean_ctor_set_tag(v___x_2226_, 5);
lean_ctor_set(v___x_2226_, 1, v___x_2233_);
lean_ctor_set(v___x_2226_, 0, v___x_2232_);
v___x_2235_ = v___x_2226_;
goto v_reusejp_2234_;
}
else
{
lean_object* v_reuseFailAlloc_2243_; 
v_reuseFailAlloc_2243_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2243_, 0, v___x_2232_);
lean_ctor_set(v_reuseFailAlloc_2243_, 1, v___x_2233_);
v___x_2235_ = v_reuseFailAlloc_2243_;
goto v_reusejp_2234_;
}
v_reusejp_2234_:
{
lean_object* v___x_2236_; lean_object* v___x_2237_; lean_object* v___x_2238_; lean_object* v___x_2239_; uint8_t v___x_2240_; lean_object* v___x_2241_; lean_object* v___x_2242_; 
v___x_2236_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2236_, 0, v___x_2235_);
lean_ctor_set(v___x_2236_, 1, v___x_2231_);
v___x_2237_ = lp_solar__verify_SolarVerify_Smt_instReprTerm_repr(v_body_2224_, v___x_2228_);
v___x_2238_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2238_, 0, v___x_2236_);
lean_ctor_set(v___x_2238_, 1, v___x_2237_);
lean_inc(v___y_2230_);
v___x_2239_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2239_, 0, v___y_2230_);
lean_ctor_set(v___x_2239_, 1, v___x_2238_);
v___x_2240_ = 0;
v___x_2241_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2241_, 0, v___x_2239_);
lean_ctor_set_uint8(v___x_2241_, sizeof(void*)*1, v___x_2240_);
v___x_2242_ = l_Repr_addAppParen(v___x_2241_, v_prec_2010_);
return v___x_2242_;
}
}
}
}
default: 
{
lean_object* v_binders_2248_; lean_object* v_body_2249_; lean_object* v___x_2251_; uint8_t v_isShared_2252_; uint8_t v_isSharedCheck_2272_; 
v_binders_2248_ = lean_ctor_get(v_x_2009_, 0);
v_body_2249_ = lean_ctor_get(v_x_2009_, 1);
v_isSharedCheck_2272_ = !lean_is_exclusive(v_x_2009_);
if (v_isSharedCheck_2272_ == 0)
{
v___x_2251_ = v_x_2009_;
v_isShared_2252_ = v_isSharedCheck_2272_;
goto v_resetjp_2250_;
}
else
{
lean_inc(v_body_2249_);
lean_inc(v_binders_2248_);
lean_dec(v_x_2009_);
v___x_2251_ = lean_box(0);
v_isShared_2252_ = v_isSharedCheck_2272_;
goto v_resetjp_2250_;
}
v_resetjp_2250_:
{
lean_object* v___x_2253_; lean_object* v___y_2255_; uint8_t v___x_2269_; 
v___x_2253_ = lean_unsigned_to_nat(1024u);
v___x_2269_ = lean_nat_dec_le(v___x_2253_, v_prec_2010_);
if (v___x_2269_ == 0)
{
lean_object* v___x_2270_; 
v___x_2270_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__4);
v___y_2255_ = v___x_2270_;
goto v___jp_2254_;
}
else
{
lean_object* v___x_2271_; 
v___x_2271_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5, &lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5_once, _init_lp_solar__verify_SolarVerify_Smt_instReprSrt_repr___closed__5);
v___y_2255_ = v___x_2271_;
goto v___jp_2254_;
}
v___jp_2254_:
{
lean_object* v___x_2256_; lean_object* v___x_2257_; lean_object* v___x_2258_; lean_object* v___x_2260_; 
v___x_2256_ = lean_box(1);
v___x_2257_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___closed__32));
v___x_2258_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1___redArg(v_binders_2248_);
if (v_isShared_2252_ == 0)
{
lean_ctor_set_tag(v___x_2251_, 5);
lean_ctor_set(v___x_2251_, 1, v___x_2258_);
lean_ctor_set(v___x_2251_, 0, v___x_2257_);
v___x_2260_ = v___x_2251_;
goto v_reusejp_2259_;
}
else
{
lean_object* v_reuseFailAlloc_2268_; 
v_reuseFailAlloc_2268_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2268_, 0, v___x_2257_);
lean_ctor_set(v_reuseFailAlloc_2268_, 1, v___x_2258_);
v___x_2260_ = v_reuseFailAlloc_2268_;
goto v_reusejp_2259_;
}
v_reusejp_2259_:
{
lean_object* v___x_2261_; lean_object* v___x_2262_; lean_object* v___x_2263_; lean_object* v___x_2264_; uint8_t v___x_2265_; lean_object* v___x_2266_; lean_object* v___x_2267_; 
v___x_2261_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2261_, 0, v___x_2260_);
lean_ctor_set(v___x_2261_, 1, v___x_2256_);
v___x_2262_ = lp_solar__verify_SolarVerify_Smt_instReprTerm_repr(v_body_2249_, v___x_2253_);
v___x_2263_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2263_, 0, v___x_2261_);
lean_ctor_set(v___x_2263_, 1, v___x_2262_);
lean_inc(v___y_2255_);
v___x_2264_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2264_, 0, v___y_2255_);
lean_ctor_set(v___x_2264_, 1, v___x_2263_);
v___x_2265_ = 0;
v___x_2266_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2266_, 0, v___x_2264_);
lean_ctor_set_uint8(v___x_2266_, sizeof(void*)*1, v___x_2265_);
v___x_2267_ = l_Repr_addAppParen(v___x_2266_, v_prec_2010_);
return v___x_2267_;
}
}
}
}
}
v___jp_2011_:
{
lean_object* v___x_2015_; lean_object* v___x_2016_; uint8_t v___x_2017_; lean_object* v___x_2018_; lean_object* v___x_2019_; 
lean_inc(v___y_2013_);
v___x_2015_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2015_, 0, v___y_2013_);
lean_ctor_set(v___x_2015_, 1, v___y_2014_);
lean_inc(v___y_2012_);
v___x_2016_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2016_, 0, v___y_2012_);
lean_ctor_set(v___x_2016_, 1, v___x_2015_);
v___x_2017_ = 0;
v___x_2018_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2018_, 0, v___x_2016_);
lean_ctor_set_uint8(v___x_2018_, sizeof(void*)*1, v___x_2017_);
v___x_2019_ = l_Repr_addAppParen(v___x_2018_, v_prec_2010_);
return v___x_2019_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_Std_Format_joinSep___at___00List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0_spec__0___lam__0(lean_object* v___y_2273_){
_start:
{
lean_object* v___x_2274_; lean_object* v___x_2275_; 
v___x_2274_ = lean_unsigned_to_nat(0u);
v___x_2275_ = lp_solar__verify_SolarVerify_Smt_instReprTerm_repr(v___y_2273_, v___x_2274_);
return v___x_2275_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprTerm_repr___boxed(lean_object* v_x_2276_, lean_object* v_prec_2277_){
_start:
{
lean_object* v_res_2278_; 
v_res_2278_ = lp_solar__verify_SolarVerify_Smt_instReprTerm_repr(v_x_2276_, v_prec_2277_);
lean_dec(v_prec_2277_);
return v_res_2278_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0(lean_object* v_a_2279_, lean_object* v_n_2280_){
_start:
{
lean_object* v___x_2281_; 
v___x_2281_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0___redArg(v_a_2279_);
return v___x_2281_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0___boxed(lean_object* v_a_2282_, lean_object* v_n_2283_){
_start:
{
lean_object* v_res_2284_; 
v_res_2284_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__0(v_a_2282_, v_n_2283_);
lean_dec(v_n_2283_);
return v_res_2284_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1(lean_object* v_a_2285_, lean_object* v_n_2286_){
_start:
{
lean_object* v___x_2287_; 
v___x_2287_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1___redArg(v_a_2285_);
return v___x_2287_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1___boxed(lean_object* v_a_2288_, lean_object* v_n_2289_){
_start:
{
lean_object* v_res_2290_; 
v_res_2290_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1(v_a_2288_, v_n_2289_);
lean_dec(v_n_2289_);
return v_res_2290_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__7(void){
_start:
{
lean_object* v___x_2310_; lean_object* v___x_2311_; 
v___x_2310_ = lean_unsigned_to_nat(8u);
v___x_2311_ = lean_nat_to_int(v___x_2310_);
return v___x_2311_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__12(void){
_start:
{
lean_object* v___x_2318_; lean_object* v___x_2319_; 
v___x_2318_ = lean_unsigned_to_nat(7u);
v___x_2319_ = lean_nat_to_int(v___x_2318_);
return v___x_2319_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__14(void){
_start:
{
lean_object* v___x_2321_; lean_object* v___x_2322_; 
v___x_2321_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__0));
v___x_2322_ = lean_string_length(v___x_2321_);
return v___x_2322_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__15(void){
_start:
{
lean_object* v___x_2323_; lean_object* v___x_2324_; 
v___x_2323_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__14, &lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__14_once, _init_lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__14);
v___x_2324_ = lean_nat_to_int(v___x_2323_);
return v___x_2324_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg(lean_object* v_x_2329_){
_start:
{
lean_object* v_name_2330_; lean_object* v_args_2331_; lean_object* v_ret_2332_; lean_object* v___x_2333_; lean_object* v___x_2334_; lean_object* v___x_2335_; lean_object* v___x_2336_; lean_object* v___x_2337_; lean_object* v___x_2338_; uint8_t v___x_2339_; lean_object* v___x_2340_; lean_object* v___x_2341_; lean_object* v___x_2342_; lean_object* v___x_2343_; lean_object* v___x_2344_; lean_object* v___x_2345_; lean_object* v___x_2346_; lean_object* v___x_2347_; lean_object* v___x_2348_; lean_object* v___x_2349_; lean_object* v___x_2350_; lean_object* v___x_2351_; lean_object* v___x_2352_; lean_object* v___x_2353_; lean_object* v___x_2354_; lean_object* v___x_2355_; lean_object* v___x_2356_; lean_object* v___x_2357_; lean_object* v___x_2358_; lean_object* v___x_2359_; lean_object* v___x_2360_; lean_object* v___x_2361_; lean_object* v___x_2362_; lean_object* v___x_2363_; lean_object* v___x_2364_; lean_object* v___x_2365_; lean_object* v___x_2366_; lean_object* v___x_2367_; lean_object* v___x_2368_; lean_object* v___x_2369_; lean_object* v___x_2370_; 
v_name_2330_ = lean_ctor_get(v_x_2329_, 0);
lean_inc_ref(v_name_2330_);
v_args_2331_ = lean_ctor_get(v_x_2329_, 1);
lean_inc(v_args_2331_);
v_ret_2332_ = lean_ctor_get(v_x_2329_, 2);
lean_inc(v_ret_2332_);
lean_dec_ref(v_x_2329_);
v___x_2333_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__5));
v___x_2334_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__6));
v___x_2335_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__7, &lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__7_once, _init_lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__7);
v___x_2336_ = l_String_quote(v_name_2330_);
v___x_2337_ = lean_alloc_ctor(3, 1, 0);
lean_ctor_set(v___x_2337_, 0, v___x_2336_);
v___x_2338_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2338_, 0, v___x_2335_);
lean_ctor_set(v___x_2338_, 1, v___x_2337_);
v___x_2339_ = 0;
v___x_2340_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2340_, 0, v___x_2338_);
lean_ctor_set_uint8(v___x_2340_, sizeof(void*)*1, v___x_2339_);
v___x_2341_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2341_, 0, v___x_2334_);
lean_ctor_set(v___x_2341_, 1, v___x_2340_);
v___x_2342_ = ((lean_object*)(lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprSrt_repr_spec__0___redArg___closed__4));
v___x_2343_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2343_, 0, v___x_2341_);
lean_ctor_set(v___x_2343_, 1, v___x_2342_);
v___x_2344_ = lean_box(1);
v___x_2345_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2345_, 0, v___x_2343_);
lean_ctor_set(v___x_2345_, 1, v___x_2344_);
v___x_2346_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__9));
v___x_2347_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2347_, 0, v___x_2345_);
lean_ctor_set(v___x_2347_, 1, v___x_2346_);
v___x_2348_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2348_, 0, v___x_2347_);
lean_ctor_set(v___x_2348_, 1, v___x_2333_);
v___x_2349_ = lean_unsigned_to_nat(0u);
v___x_2350_ = lp_solar__verify_List_repr___at___00SolarVerify_Smt_instReprTerm_repr_spec__1___redArg(v_args_2331_);
v___x_2351_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2351_, 0, v___x_2335_);
lean_ctor_set(v___x_2351_, 1, v___x_2350_);
v___x_2352_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2352_, 0, v___x_2351_);
lean_ctor_set_uint8(v___x_2352_, sizeof(void*)*1, v___x_2339_);
v___x_2353_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2353_, 0, v___x_2348_);
lean_ctor_set(v___x_2353_, 1, v___x_2352_);
v___x_2354_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2354_, 0, v___x_2353_);
lean_ctor_set(v___x_2354_, 1, v___x_2342_);
v___x_2355_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2355_, 0, v___x_2354_);
lean_ctor_set(v___x_2355_, 1, v___x_2344_);
v___x_2356_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__11));
v___x_2357_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2357_, 0, v___x_2355_);
lean_ctor_set(v___x_2357_, 1, v___x_2356_);
v___x_2358_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2358_, 0, v___x_2357_);
lean_ctor_set(v___x_2358_, 1, v___x_2333_);
v___x_2359_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__12, &lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__12_once, _init_lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__12);
v___x_2360_ = lp_solar__verify_SolarVerify_Smt_instReprSrt_repr(v_ret_2332_, v___x_2349_);
v___x_2361_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2361_, 0, v___x_2359_);
lean_ctor_set(v___x_2361_, 1, v___x_2360_);
v___x_2362_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2362_, 0, v___x_2361_);
lean_ctor_set_uint8(v___x_2362_, sizeof(void*)*1, v___x_2339_);
v___x_2363_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2363_, 0, v___x_2358_);
lean_ctor_set(v___x_2363_, 1, v___x_2362_);
v___x_2364_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__15, &lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__15_once, _init_lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__15);
v___x_2365_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__16));
v___x_2366_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2366_, 0, v___x_2365_);
lean_ctor_set(v___x_2366_, 1, v___x_2363_);
v___x_2367_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg___closed__17));
v___x_2368_ = lean_alloc_ctor(5, 2, 0);
lean_ctor_set(v___x_2368_, 0, v___x_2366_);
lean_ctor_set(v___x_2368_, 1, v___x_2367_);
v___x_2369_ = lean_alloc_ctor(4, 2, 0);
lean_ctor_set(v___x_2369_, 0, v___x_2364_);
lean_ctor_set(v___x_2369_, 1, v___x_2368_);
v___x_2370_ = lean_alloc_ctor(6, 1, 1);
lean_ctor_set(v___x_2370_, 0, v___x_2369_);
lean_ctor_set_uint8(v___x_2370_, sizeof(void*)*1, v___x_2339_);
return v___x_2370_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr(lean_object* v_x_2371_, lean_object* v_prec_2372_){
_start:
{
lean_object* v___x_2373_; 
v___x_2373_ = lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___redArg(v_x_2371_);
return v___x_2373_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_instReprDecl_repr___boxed(lean_object* v_x_2374_, lean_object* v_prec_2375_){
_start:
{
lean_object* v_res_2376_; 
v_res_2376_ = lp_solar__verify_SolarVerify_Smt_instReprDecl_repr(v_x_2374_, v_prec_2375_);
lean_dec(v_prec_2375_);
return v_res_2376_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_intDiv(lean_object* v_env_2386_, lean_object* v_a_2387_, lean_object* v_b_2388_){
_start:
{
lean_object* v___x_2389_; uint8_t v___x_2390_; 
v___x_2389_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0, &lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0_once, _init_lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0);
v___x_2390_ = lean_int_dec_eq(v_b_2388_, v___x_2389_);
if (v___x_2390_ == 0)
{
lean_object* v___x_2391_; 
lean_dec_ref(v_env_2386_);
v___x_2391_ = lean_int_ediv(v_a_2387_, v_b_2388_);
lean_dec(v_a_2387_);
return v___x_2391_;
}
else
{
lean_object* v_divZero_2392_; lean_object* v___x_2393_; 
v_divZero_2392_ = lean_ctor_get(v_env_2386_, 1);
lean_inc_ref(v_divZero_2392_);
lean_dec_ref(v_env_2386_);
v___x_2393_ = lean_apply_1(v_divZero_2392_, v_a_2387_);
return v___x_2393_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_intDiv___boxed(lean_object* v_env_2394_, lean_object* v_a_2395_, lean_object* v_b_2396_){
_start:
{
lean_object* v_res_2397_; 
v_res_2397_ = lp_solar__verify_SolarVerify_Smt_intDiv(v_env_2394_, v_a_2395_, v_b_2396_);
lean_dec(v_b_2396_);
return v_res_2397_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_intMod(lean_object* v_env_2398_, lean_object* v_a_2399_, lean_object* v_b_2400_){
_start:
{
lean_object* v___x_2401_; uint8_t v___x_2402_; 
v___x_2401_ = lean_obj_once(&lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0, &lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0_once, _init_lp_solar__verify_SolarVerify_Smt_Srt_default___closed__0);
v___x_2402_ = lean_int_dec_eq(v_b_2400_, v___x_2401_);
if (v___x_2402_ == 0)
{
lean_object* v___x_2403_; 
lean_dec_ref(v_env_2398_);
v___x_2403_ = lean_int_emod(v_a_2399_, v_b_2400_);
lean_dec(v_a_2399_);
return v___x_2403_;
}
else
{
lean_object* v_modZero_2404_; lean_object* v___x_2405_; 
v_modZero_2404_ = lean_ctor_get(v_env_2398_, 2);
lean_inc_ref(v_modZero_2404_);
lean_dec_ref(v_env_2398_);
v___x_2405_ = lean_apply_1(v_modZero_2404_, v_a_2399_);
return v___x_2405_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_intMod___boxed(lean_object* v_env_2406_, lean_object* v_a_2407_, lean_object* v_b_2408_){
_start:
{
lean_object* v_res_2409_; 
v_res_2409_ = lp_solar__verify_SolarVerify_Smt_intMod(v_env_2406_, v_a_2407_, v_b_2408_);
lean_dec(v_b_2408_);
return v_res_2409_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_chain___redArg(lean_object* v_r_2410_, lean_object* v_x_2411_, lean_object* v_x_2412_){
_start:
{
if (lean_obj_tag(v_x_2412_) == 0)
{
uint8_t v___x_2413_; 
lean_dec(v_x_2411_);
lean_dec_ref(v_r_2410_);
v___x_2413_ = 1;
return v___x_2413_;
}
else
{
lean_object* v_head_2414_; lean_object* v_tail_2415_; lean_object* v___x_2416_; uint8_t v___x_2417_; 
v_head_2414_ = lean_ctor_get(v_x_2412_, 0);
lean_inc_n(v_head_2414_, 2);
v_tail_2415_ = lean_ctor_get(v_x_2412_, 1);
lean_inc(v_tail_2415_);
lean_dec_ref_known(v_x_2412_, 2);
lean_inc_ref(v_r_2410_);
v___x_2416_ = lean_apply_2(v_r_2410_, v_x_2411_, v_head_2414_);
v___x_2417_ = lean_unbox(v___x_2416_);
if (v___x_2417_ == 0)
{
uint8_t v___x_2418_; 
lean_dec(v_tail_2415_);
lean_dec(v_head_2414_);
lean_dec_ref(v_r_2410_);
v___x_2418_ = lean_unbox(v___x_2416_);
return v___x_2418_;
}
else
{
v_x_2411_ = v_head_2414_;
v_x_2412_ = v_tail_2415_;
goto _start;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_chain___redArg___boxed(lean_object* v_r_2420_, lean_object* v_x_2421_, lean_object* v_x_2422_){
_start:
{
uint8_t v_res_2423_; lean_object* v_r_2424_; 
v_res_2423_ = lp_solar__verify_SolarVerify_Smt_chain___redArg(v_r_2420_, v_x_2421_, v_x_2422_);
v_r_2424_ = lean_box(v_res_2423_);
return v_r_2424_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_chain(lean_object* v_00_u03b1_2425_, lean_object* v_r_2426_, lean_object* v_x_2427_, lean_object* v_x_2428_){
_start:
{
uint8_t v___x_2429_; 
v___x_2429_ = lp_solar__verify_SolarVerify_Smt_chain___redArg(v_r_2426_, v_x_2427_, v_x_2428_);
return v___x_2429_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_chain___boxed(lean_object* v_00_u03b1_2430_, lean_object* v_r_2431_, lean_object* v_x_2432_, lean_object* v_x_2433_){
_start:
{
uint8_t v_res_2434_; lean_object* v_r_2435_; 
v_res_2434_ = lp_solar__verify_SolarVerify_Smt_chain(v_00_u03b1_2430_, v_r_2431_, v_x_2432_, v_x_2433_);
v_r_2435_ = lean_box(v_res_2434_);
return v_r_2435_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0___redArg(lean_object* v___x_2436_, lean_object* v_x_2437_){
_start:
{
if (lean_obj_tag(v_x_2437_) == 0)
{
uint8_t v___x_2438_; 
lean_dec_ref(v___x_2436_);
v___x_2438_ = 1;
return v___x_2438_;
}
else
{
lean_object* v_head_2439_; lean_object* v_tail_2440_; lean_object* v___x_2441_; uint8_t v___x_2442_; 
v_head_2439_ = lean_ctor_get(v_x_2437_, 0);
lean_inc(v_head_2439_);
v_tail_2440_ = lean_ctor_get(v_x_2437_, 1);
lean_inc(v_tail_2440_);
lean_dec_ref_known(v_x_2437_, 2);
lean_inc_ref(v___x_2436_);
v___x_2441_ = lean_apply_1(v___x_2436_, v_head_2439_);
v___x_2442_ = lean_unbox(v___x_2441_);
if (v___x_2442_ == 0)
{
uint8_t v___x_2443_; 
lean_dec(v_tail_2440_);
lean_dec_ref(v___x_2436_);
v___x_2443_ = lean_unbox(v___x_2441_);
return v___x_2443_;
}
else
{
v_x_2437_ = v_tail_2440_;
goto _start;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0___redArg___boxed(lean_object* v___x_2445_, lean_object* v_x_2446_){
_start:
{
uint8_t v_res_2447_; lean_object* v_r_2448_; 
v_res_2447_ = lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0___redArg(v___x_2445_, v_x_2446_);
v_r_2448_ = lean_box(v_res_2447_);
return v_r_2448_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_pairwise___redArg(lean_object* v_r_2449_, lean_object* v_x_2450_){
_start:
{
if (lean_obj_tag(v_x_2450_) == 0)
{
uint8_t v___x_2451_; 
lean_dec_ref(v_r_2449_);
v___x_2451_ = 1;
return v___x_2451_;
}
else
{
lean_object* v_head_2452_; lean_object* v_tail_2453_; lean_object* v___x_2454_; uint8_t v___x_2455_; 
v_head_2452_ = lean_ctor_get(v_x_2450_, 0);
lean_inc(v_head_2452_);
v_tail_2453_ = lean_ctor_get(v_x_2450_, 1);
lean_inc_n(v_tail_2453_, 2);
lean_dec_ref_known(v_x_2450_, 2);
lean_inc_ref(v_r_2449_);
v___x_2454_ = lean_apply_1(v_r_2449_, v_head_2452_);
v___x_2455_ = lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0___redArg(v___x_2454_, v_tail_2453_);
if (v___x_2455_ == 0)
{
lean_dec(v_tail_2453_);
lean_dec_ref(v_r_2449_);
return v___x_2455_;
}
else
{
v_x_2450_ = v_tail_2453_;
goto _start;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_pairwise___redArg___boxed(lean_object* v_r_2457_, lean_object* v_x_2458_){
_start:
{
uint8_t v_res_2459_; lean_object* v_r_2460_; 
v_res_2459_ = lp_solar__verify_SolarVerify_Smt_pairwise___redArg(v_r_2457_, v_x_2458_);
v_r_2460_ = lean_box(v_res_2459_);
return v_r_2460_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Smt_pairwise(lean_object* v_00_u03b1_2461_, lean_object* v_r_2462_, lean_object* v_x_2463_){
_start:
{
uint8_t v___x_2464_; 
v___x_2464_ = lp_solar__verify_SolarVerify_Smt_pairwise___redArg(v_r_2462_, v_x_2463_);
return v___x_2464_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_pairwise___boxed(lean_object* v_00_u03b1_2465_, lean_object* v_r_2466_, lean_object* v_x_2467_){
_start:
{
uint8_t v_res_2468_; lean_object* v_r_2469_; 
v_res_2468_ = lp_solar__verify_SolarVerify_Smt_pairwise(v_00_u03b1_2465_, v_r_2466_, v_x_2467_);
v_r_2469_ = lean_box(v_res_2468_);
return v_r_2469_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0(lean_object* v_00_u03b1_2470_, lean_object* v___x_2471_, lean_object* v_x_2472_){
_start:
{
uint8_t v___x_2473_; 
v___x_2473_ = lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0___redArg(v___x_2471_, v_x_2472_);
return v___x_2473_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0___boxed(lean_object* v_00_u03b1_2474_, lean_object* v___x_2475_, lean_object* v_x_2476_){
_start:
{
uint8_t v_res_2477_; lean_object* v_r_2478_; 
v_res_2477_ = lp_solar__verify_List_all___at___00SolarVerify_Smt_pairwise_spec__0(v_00_u03b1_2474_, v___x_2475_, v_x_2476_);
v_r_2478_ = lean_box(v_res_2477_);
return v_r_2478_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_ints(lean_object* v_x_2481_){
_start:
{
if (lean_obj_tag(v_x_2481_) == 0)
{
lean_object* v___x_2482_; 
v___x_2482_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_ints___closed__0));
return v___x_2482_;
}
else
{
lean_object* v_head_2483_; lean_object* v_tail_2484_; lean_object* v___x_2486_; uint8_t v_isShared_2487_; uint8_t v_isSharedCheck_2504_; 
v_head_2483_ = lean_ctor_get(v_x_2481_, 0);
v_tail_2484_ = lean_ctor_get(v_x_2481_, 1);
v_isSharedCheck_2504_ = !lean_is_exclusive(v_x_2481_);
if (v_isSharedCheck_2504_ == 0)
{
v___x_2486_ = v_x_2481_;
v_isShared_2487_ = v_isSharedCheck_2504_;
goto v_resetjp_2485_;
}
else
{
lean_inc(v_tail_2484_);
lean_inc(v_head_2483_);
lean_dec(v_x_2481_);
v___x_2486_ = lean_box(0);
v_isShared_2487_ = v_isSharedCheck_2504_;
goto v_resetjp_2485_;
}
v_resetjp_2485_:
{
lean_object* v___x_2488_; lean_object* v___x_2489_; 
v___x_2488_ = lean_box(1);
v___x_2489_ = lp_solar__verify_SolarVerify_Smt_Val_as(v_head_2483_, v___x_2488_);
lean_dec(v_head_2483_);
if (lean_obj_tag(v___x_2489_) == 0)
{
lean_object* v___x_2490_; 
lean_del_object(v___x_2486_);
lean_dec(v_tail_2484_);
v___x_2490_ = lean_box(0);
return v___x_2490_;
}
else
{
lean_object* v_val_2491_; lean_object* v___x_2492_; 
v_val_2491_ = lean_ctor_get(v___x_2489_, 0);
lean_inc(v_val_2491_);
lean_dec_ref_known(v___x_2489_, 1);
v___x_2492_ = lp_solar__verify_SolarVerify_Smt_ints(v_tail_2484_);
if (lean_obj_tag(v___x_2492_) == 0)
{
lean_dec(v_val_2491_);
lean_del_object(v___x_2486_);
return v___x_2492_;
}
else
{
lean_object* v_val_2493_; lean_object* v___x_2495_; uint8_t v_isShared_2496_; uint8_t v_isSharedCheck_2503_; 
v_val_2493_ = lean_ctor_get(v___x_2492_, 0);
v_isSharedCheck_2503_ = !lean_is_exclusive(v___x_2492_);
if (v_isSharedCheck_2503_ == 0)
{
v___x_2495_ = v___x_2492_;
v_isShared_2496_ = v_isSharedCheck_2503_;
goto v_resetjp_2494_;
}
else
{
lean_inc(v_val_2493_);
lean_dec(v___x_2492_);
v___x_2495_ = lean_box(0);
v_isShared_2496_ = v_isSharedCheck_2503_;
goto v_resetjp_2494_;
}
v_resetjp_2494_:
{
lean_object* v___x_2498_; 
if (v_isShared_2487_ == 0)
{
lean_ctor_set(v___x_2486_, 1, v_val_2493_);
lean_ctor_set(v___x_2486_, 0, v_val_2491_);
v___x_2498_ = v___x_2486_;
goto v_reusejp_2497_;
}
else
{
lean_object* v_reuseFailAlloc_2502_; 
v_reuseFailAlloc_2502_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2502_, 0, v_val_2491_);
lean_ctor_set(v_reuseFailAlloc_2502_, 1, v_val_2493_);
v___x_2498_ = v_reuseFailAlloc_2502_;
goto v_reusejp_2497_;
}
v_reusejp_2497_:
{
lean_object* v___x_2500_; 
if (v_isShared_2496_ == 0)
{
lean_ctor_set(v___x_2495_, 0, v___x_2498_);
v___x_2500_ = v___x_2495_;
goto v_reusejp_2499_;
}
else
{
lean_object* v_reuseFailAlloc_2501_; 
v_reuseFailAlloc_2501_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2501_, 0, v___x_2498_);
v___x_2500_ = v_reuseFailAlloc_2501_;
goto v_reusejp_2499_;
}
v_reusejp_2499_:
{
return v___x_2500_;
}
}
}
}
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bools(lean_object* v_x_2507_){
_start:
{
if (lean_obj_tag(v_x_2507_) == 0)
{
lean_object* v___x_2508_; 
v___x_2508_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_bools___closed__0));
return v___x_2508_;
}
else
{
lean_object* v_head_2509_; lean_object* v_tail_2510_; lean_object* v___x_2512_; uint8_t v_isShared_2513_; uint8_t v_isSharedCheck_2530_; 
v_head_2509_ = lean_ctor_get(v_x_2507_, 0);
v_tail_2510_ = lean_ctor_get(v_x_2507_, 1);
v_isSharedCheck_2530_ = !lean_is_exclusive(v_x_2507_);
if (v_isSharedCheck_2530_ == 0)
{
v___x_2512_ = v_x_2507_;
v_isShared_2513_ = v_isSharedCheck_2530_;
goto v_resetjp_2511_;
}
else
{
lean_inc(v_tail_2510_);
lean_inc(v_head_2509_);
lean_dec(v_x_2507_);
v___x_2512_ = lean_box(0);
v_isShared_2513_ = v_isSharedCheck_2530_;
goto v_resetjp_2511_;
}
v_resetjp_2511_:
{
lean_object* v___x_2514_; lean_object* v___x_2515_; 
v___x_2514_ = lean_box(0);
v___x_2515_ = lp_solar__verify_SolarVerify_Smt_Val_as(v_head_2509_, v___x_2514_);
lean_dec(v_head_2509_);
if (lean_obj_tag(v___x_2515_) == 0)
{
lean_object* v___x_2516_; 
lean_del_object(v___x_2512_);
lean_dec(v_tail_2510_);
v___x_2516_ = lean_box(0);
return v___x_2516_;
}
else
{
lean_object* v_val_2517_; lean_object* v___x_2518_; 
v_val_2517_ = lean_ctor_get(v___x_2515_, 0);
lean_inc(v_val_2517_);
lean_dec_ref_known(v___x_2515_, 1);
v___x_2518_ = lp_solar__verify_SolarVerify_Smt_bools(v_tail_2510_);
if (lean_obj_tag(v___x_2518_) == 0)
{
lean_dec(v_val_2517_);
lean_del_object(v___x_2512_);
return v___x_2518_;
}
else
{
lean_object* v_val_2519_; lean_object* v___x_2521_; uint8_t v_isShared_2522_; uint8_t v_isSharedCheck_2529_; 
v_val_2519_ = lean_ctor_get(v___x_2518_, 0);
v_isSharedCheck_2529_ = !lean_is_exclusive(v___x_2518_);
if (v_isSharedCheck_2529_ == 0)
{
v___x_2521_ = v___x_2518_;
v_isShared_2522_ = v_isSharedCheck_2529_;
goto v_resetjp_2520_;
}
else
{
lean_inc(v_val_2519_);
lean_dec(v___x_2518_);
v___x_2521_ = lean_box(0);
v_isShared_2522_ = v_isSharedCheck_2529_;
goto v_resetjp_2520_;
}
v_resetjp_2520_:
{
lean_object* v___x_2524_; 
if (v_isShared_2513_ == 0)
{
lean_ctor_set(v___x_2512_, 1, v_val_2519_);
lean_ctor_set(v___x_2512_, 0, v_val_2517_);
v___x_2524_ = v___x_2512_;
goto v_reusejp_2523_;
}
else
{
lean_object* v_reuseFailAlloc_2528_; 
v_reuseFailAlloc_2528_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2528_, 0, v_val_2517_);
lean_ctor_set(v_reuseFailAlloc_2528_, 1, v_val_2519_);
v___x_2524_ = v_reuseFailAlloc_2528_;
goto v_reusejp_2523_;
}
v_reusejp_2523_:
{
lean_object* v___x_2526_; 
if (v_isShared_2522_ == 0)
{
lean_ctor_set(v___x_2521_, 0, v___x_2524_);
v___x_2526_ = v___x_2521_;
goto v_reusejp_2525_;
}
else
{
lean_object* v_reuseFailAlloc_2527_; 
v_reuseFailAlloc_2527_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2527_, 0, v___x_2524_);
v___x_2526_ = v_reuseFailAlloc_2527_;
goto v_reusejp_2525_;
}
v_reusejp_2525_:
{
return v___x_2526_;
}
}
}
}
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_sameSort(lean_object* v_s_2533_, lean_object* v_x_2534_){
_start:
{
if (lean_obj_tag(v_x_2534_) == 0)
{
lean_object* v___x_2535_; 
v___x_2535_ = ((lean_object*)(lp_solar__verify_SolarVerify_Smt_sameSort___closed__0));
return v___x_2535_;
}
else
{
lean_object* v_head_2536_; lean_object* v_tail_2537_; lean_object* v___x_2539_; uint8_t v_isShared_2540_; uint8_t v_isSharedCheck_2556_; 
v_head_2536_ = lean_ctor_get(v_x_2534_, 0);
v_tail_2537_ = lean_ctor_get(v_x_2534_, 1);
v_isSharedCheck_2556_ = !lean_is_exclusive(v_x_2534_);
if (v_isSharedCheck_2556_ == 0)
{
v___x_2539_ = v_x_2534_;
v_isShared_2540_ = v_isSharedCheck_2556_;
goto v_resetjp_2538_;
}
else
{
lean_inc(v_tail_2537_);
lean_inc(v_head_2536_);
lean_dec(v_x_2534_);
v___x_2539_ = lean_box(0);
v_isShared_2540_ = v_isSharedCheck_2556_;
goto v_resetjp_2538_;
}
v_resetjp_2538_:
{
lean_object* v___x_2541_; 
v___x_2541_ = lp_solar__verify_SolarVerify_Smt_Val_as(v_head_2536_, v_s_2533_);
lean_dec(v_head_2536_);
if (lean_obj_tag(v___x_2541_) == 0)
{
lean_object* v___x_2542_; 
lean_del_object(v___x_2539_);
lean_dec(v_tail_2537_);
v___x_2542_ = lean_box(0);
return v___x_2542_;
}
else
{
lean_object* v_val_2543_; lean_object* v___x_2544_; 
v_val_2543_ = lean_ctor_get(v___x_2541_, 0);
lean_inc(v_val_2543_);
lean_dec_ref_known(v___x_2541_, 1);
v___x_2544_ = lp_solar__verify_SolarVerify_Smt_sameSort(v_s_2533_, v_tail_2537_);
if (lean_obj_tag(v___x_2544_) == 0)
{
lean_dec(v_val_2543_);
lean_del_object(v___x_2539_);
return v___x_2544_;
}
else
{
lean_object* v_val_2545_; lean_object* v___x_2547_; uint8_t v_isShared_2548_; uint8_t v_isSharedCheck_2555_; 
v_val_2545_ = lean_ctor_get(v___x_2544_, 0);
v_isSharedCheck_2555_ = !lean_is_exclusive(v___x_2544_);
if (v_isSharedCheck_2555_ == 0)
{
v___x_2547_ = v___x_2544_;
v_isShared_2548_ = v_isSharedCheck_2555_;
goto v_resetjp_2546_;
}
else
{
lean_inc(v_val_2545_);
lean_dec(v___x_2544_);
v___x_2547_ = lean_box(0);
v_isShared_2548_ = v_isSharedCheck_2555_;
goto v_resetjp_2546_;
}
v_resetjp_2546_:
{
lean_object* v___x_2550_; 
if (v_isShared_2540_ == 0)
{
lean_ctor_set(v___x_2539_, 1, v_val_2545_);
lean_ctor_set(v___x_2539_, 0, v_val_2543_);
v___x_2550_ = v___x_2539_;
goto v_reusejp_2549_;
}
else
{
lean_object* v_reuseFailAlloc_2554_; 
v_reuseFailAlloc_2554_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2554_, 0, v_val_2543_);
lean_ctor_set(v_reuseFailAlloc_2554_, 1, v_val_2545_);
v___x_2550_ = v_reuseFailAlloc_2554_;
goto v_reusejp_2549_;
}
v_reusejp_2549_:
{
lean_object* v___x_2552_; 
if (v_isShared_2548_ == 0)
{
lean_ctor_set(v___x_2547_, 0, v___x_2550_);
v___x_2552_ = v___x_2547_;
goto v_reusejp_2551_;
}
else
{
lean_object* v_reuseFailAlloc_2553_; 
v_reuseFailAlloc_2553_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2553_, 0, v___x_2550_);
v___x_2552_ = v_reuseFailAlloc_2553_;
goto v_reusejp_2551_;
}
v_reusejp_2551_:
{
return v___x_2552_;
}
}
}
}
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_sameSort___boxed(lean_object* v_s_2557_, lean_object* v_x_2558_){
_start:
{
lean_object* v_res_2559_; 
v_res_2559_ = lp_solar__verify_SolarVerify_Smt_sameSort(v_s_2557_, v_x_2558_);
lean_dec(v_s_2557_);
return v_res_2559_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bvs(lean_object* v_x_2560_){
_start:
{
if (lean_obj_tag(v_x_2560_) == 0)
{
lean_object* v___x_2561_; 
v___x_2561_ = lean_box(0);
return v___x_2561_;
}
else
{
lean_object* v_head_2562_; lean_object* v_tail_2563_; lean_object* v___x_2565_; uint8_t v_isShared_2566_; uint8_t v_isSharedCheck_2599_; 
v_head_2562_ = lean_ctor_get(v_x_2560_, 0);
v_tail_2563_ = lean_ctor_get(v_x_2560_, 1);
v_isSharedCheck_2599_ = !lean_is_exclusive(v_x_2560_);
if (v_isSharedCheck_2599_ == 0)
{
v___x_2565_ = v_x_2560_;
v_isShared_2566_ = v_isSharedCheck_2599_;
goto v_resetjp_2564_;
}
else
{
lean_inc(v_tail_2563_);
lean_inc(v_head_2562_);
lean_dec(v_x_2560_);
v___x_2565_ = lean_box(0);
v_isShared_2566_ = v_isSharedCheck_2599_;
goto v_resetjp_2564_;
}
v_resetjp_2564_:
{
lean_object* v___x_2567_; 
v___x_2567_ = lp_solar__verify_SolarVerify_Smt_Val_bv_x3f(v_head_2562_);
if (lean_obj_tag(v___x_2567_) == 0)
{
lean_object* v___x_2568_; 
lean_del_object(v___x_2565_);
lean_dec(v_tail_2563_);
v___x_2568_ = lean_box(0);
return v___x_2568_;
}
else
{
lean_object* v_val_2569_; lean_object* v___x_2571_; uint8_t v_isShared_2572_; uint8_t v_isSharedCheck_2598_; 
v_val_2569_ = lean_ctor_get(v___x_2567_, 0);
v_isSharedCheck_2598_ = !lean_is_exclusive(v___x_2567_);
if (v_isSharedCheck_2598_ == 0)
{
v___x_2571_ = v___x_2567_;
v_isShared_2572_ = v_isSharedCheck_2598_;
goto v_resetjp_2570_;
}
else
{
lean_inc(v_val_2569_);
lean_dec(v___x_2567_);
v___x_2571_ = lean_box(0);
v_isShared_2572_ = v_isSharedCheck_2598_;
goto v_resetjp_2570_;
}
v_resetjp_2570_:
{
lean_object* v_fst_2573_; lean_object* v_snd_2574_; lean_object* v___x_2576_; uint8_t v_isShared_2577_; uint8_t v_isSharedCheck_2597_; 
v_fst_2573_ = lean_ctor_get(v_val_2569_, 0);
v_snd_2574_ = lean_ctor_get(v_val_2569_, 1);
v_isSharedCheck_2597_ = !lean_is_exclusive(v_val_2569_);
if (v_isSharedCheck_2597_ == 0)
{
v___x_2576_ = v_val_2569_;
v_isShared_2577_ = v_isSharedCheck_2597_;
goto v_resetjp_2575_;
}
else
{
lean_inc(v_snd_2574_);
lean_inc(v_fst_2573_);
lean_dec(v_val_2569_);
v___x_2576_ = lean_box(0);
v_isShared_2577_ = v_isSharedCheck_2597_;
goto v_resetjp_2575_;
}
v_resetjp_2575_:
{
lean_object* v___x_2579_; 
lean_inc(v_fst_2573_);
if (v_isShared_2572_ == 0)
{
lean_ctor_set_tag(v___x_2571_, 2);
lean_ctor_set(v___x_2571_, 0, v_fst_2573_);
v___x_2579_ = v___x_2571_;
goto v_reusejp_2578_;
}
else
{
lean_object* v_reuseFailAlloc_2596_; 
v_reuseFailAlloc_2596_ = lean_alloc_ctor(2, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2596_, 0, v_fst_2573_);
v___x_2579_ = v_reuseFailAlloc_2596_;
goto v_reusejp_2578_;
}
v_reusejp_2578_:
{
lean_object* v___x_2580_; 
v___x_2580_ = lp_solar__verify_SolarVerify_Smt_sameSort(v___x_2579_, v_tail_2563_);
lean_dec_ref(v___x_2579_);
if (lean_obj_tag(v___x_2580_) == 0)
{
lean_object* v___x_2581_; 
lean_del_object(v___x_2576_);
lean_dec(v_snd_2574_);
lean_dec(v_fst_2573_);
lean_del_object(v___x_2565_);
v___x_2581_ = lean_box(0);
return v___x_2581_;
}
else
{
lean_object* v_val_2582_; lean_object* v___x_2584_; uint8_t v_isShared_2585_; uint8_t v_isSharedCheck_2595_; 
v_val_2582_ = lean_ctor_get(v___x_2580_, 0);
v_isSharedCheck_2595_ = !lean_is_exclusive(v___x_2580_);
if (v_isSharedCheck_2595_ == 0)
{
v___x_2584_ = v___x_2580_;
v_isShared_2585_ = v_isSharedCheck_2595_;
goto v_resetjp_2583_;
}
else
{
lean_inc(v_val_2582_);
lean_dec(v___x_2580_);
v___x_2584_ = lean_box(0);
v_isShared_2585_ = v_isSharedCheck_2595_;
goto v_resetjp_2583_;
}
v_resetjp_2583_:
{
lean_object* v___x_2587_; 
if (v_isShared_2566_ == 0)
{
lean_ctor_set(v___x_2565_, 1, v_val_2582_);
lean_ctor_set(v___x_2565_, 0, v_snd_2574_);
v___x_2587_ = v___x_2565_;
goto v_reusejp_2586_;
}
else
{
lean_object* v_reuseFailAlloc_2594_; 
v_reuseFailAlloc_2594_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2594_, 0, v_snd_2574_);
lean_ctor_set(v_reuseFailAlloc_2594_, 1, v_val_2582_);
v___x_2587_ = v_reuseFailAlloc_2594_;
goto v_reusejp_2586_;
}
v_reusejp_2586_:
{
lean_object* v___x_2589_; 
if (v_isShared_2577_ == 0)
{
lean_ctor_set(v___x_2576_, 1, v___x_2587_);
v___x_2589_ = v___x_2576_;
goto v_reusejp_2588_;
}
else
{
lean_object* v_reuseFailAlloc_2593_; 
v_reuseFailAlloc_2593_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2593_, 0, v_fst_2573_);
lean_ctor_set(v_reuseFailAlloc_2593_, 1, v___x_2587_);
v___x_2589_ = v_reuseFailAlloc_2593_;
goto v_reusejp_2588_;
}
v_reusejp_2588_:
{
lean_object* v___x_2591_; 
if (v_isShared_2585_ == 0)
{
lean_ctor_set(v___x_2584_, 0, v___x_2589_);
v___x_2591_ = v___x_2584_;
goto v_reusejp_2590_;
}
else
{
lean_object* v_reuseFailAlloc_2592_; 
v_reuseFailAlloc_2592_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2592_, 0, v___x_2589_);
v___x_2591_ = v_reuseFailAlloc_2592_;
goto v_reusejp_2590_;
}
v_reusejp_2590_:
{
return v___x_2591_;
}
}
}
}
}
}
}
}
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00SolarVerify_Smt_foldl2_spec__0___redArg(lean_object* v_f_2600_, lean_object* v_x_2601_, lean_object* v_x_2602_){
_start:
{
if (lean_obj_tag(v_x_2602_) == 0)
{
lean_dec(v_f_2600_);
return v_x_2601_;
}
else
{
lean_object* v_head_2603_; lean_object* v_tail_2604_; lean_object* v___x_2605_; 
v_head_2603_ = lean_ctor_get(v_x_2602_, 0);
lean_inc(v_head_2603_);
v_tail_2604_ = lean_ctor_get(v_x_2602_, 1);
lean_inc(v_tail_2604_);
lean_dec_ref_known(v_x_2602_, 2);
lean_inc(v_f_2600_);
v___x_2605_ = lean_apply_2(v_f_2600_, v_x_2601_, v_head_2603_);
v_x_2601_ = v___x_2605_;
v_x_2602_ = v_tail_2604_;
goto _start;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_foldl2___redArg(lean_object* v_f_2607_, lean_object* v_x_2608_){
_start:
{
if (lean_obj_tag(v_x_2608_) == 1)
{
lean_object* v_tail_2609_; 
v_tail_2609_ = lean_ctor_get(v_x_2608_, 1);
lean_inc(v_tail_2609_);
if (lean_obj_tag(v_tail_2609_) == 1)
{
lean_object* v_head_2610_; lean_object* v_head_2611_; lean_object* v_tail_2612_; lean_object* v___x_2613_; lean_object* v___x_2614_; lean_object* v___x_2615_; 
v_head_2610_ = lean_ctor_get(v_x_2608_, 0);
lean_inc(v_head_2610_);
lean_dec_ref_known(v_x_2608_, 2);
v_head_2611_ = lean_ctor_get(v_tail_2609_, 0);
lean_inc(v_head_2611_);
v_tail_2612_ = lean_ctor_get(v_tail_2609_, 1);
lean_inc(v_tail_2612_);
lean_dec_ref_known(v_tail_2609_, 2);
lean_inc(v_f_2607_);
v___x_2613_ = lean_apply_2(v_f_2607_, v_head_2610_, v_head_2611_);
v___x_2614_ = lp_solar__verify_List_foldl___at___00SolarVerify_Smt_foldl2_spec__0___redArg(v_f_2607_, v___x_2613_, v_tail_2612_);
v___x_2615_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v___x_2615_, 0, v___x_2614_);
return v___x_2615_;
}
else
{
lean_object* v___x_2616_; 
lean_dec(v_tail_2609_);
lean_dec_ref_known(v_x_2608_, 2);
lean_dec(v_f_2607_);
v___x_2616_ = lean_box(0);
return v___x_2616_;
}
}
else
{
lean_object* v___x_2617_; 
lean_dec(v_x_2608_);
lean_dec(v_f_2607_);
v___x_2617_ = lean_box(0);
return v___x_2617_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_foldl2(lean_object* v_00_u03b1_2618_, lean_object* v_f_2619_, lean_object* v_x_2620_){
_start:
{
lean_object* v___x_2621_; 
v___x_2621_ = lp_solar__verify_SolarVerify_Smt_foldl2___redArg(v_f_2619_, v_x_2620_);
return v___x_2621_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00SolarVerify_Smt_foldl2_spec__0(lean_object* v_00_u03b1_2622_, lean_object* v_f_2623_, lean_object* v_x_2624_, lean_object* v_x_2625_){
_start:
{
lean_object* v___x_2626_; 
v___x_2626_ = lp_solar__verify_List_foldl___at___00SolarVerify_Smt_foldl2_spec__0___redArg(v_f_2623_, v_x_2624_, v_x_2625_);
return v___x_2626_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_List_foldl___at___00SolarVerify_Smt_impliesChain_spec__0(uint8_t v_x_2627_, lean_object* v_x_2628_){
_start:
{
if (lean_obj_tag(v_x_2628_) == 0)
{
return v_x_2627_;
}
else
{
lean_object* v_head_2629_; uint8_t v___x_2630_; 
v_head_2629_ = lean_ctor_get(v_x_2628_, 0);
v___x_2630_ = lean_unbox(v_head_2629_);
if (v___x_2630_ == 0)
{
lean_object* v_tail_2631_; uint8_t v___x_2632_; 
v_tail_2631_ = lean_ctor_get(v_x_2628_, 1);
v___x_2632_ = 1;
v_x_2627_ = v___x_2632_;
v_x_2628_ = v_tail_2631_;
goto _start;
}
else
{
lean_object* v_tail_2634_; 
v_tail_2634_ = lean_ctor_get(v_x_2628_, 1);
v_x_2628_ = v_tail_2634_;
goto _start;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_foldl___at___00SolarVerify_Smt_impliesChain_spec__0___boxed(lean_object* v_x_2636_, lean_object* v_x_2637_){
_start:
{
uint8_t v_x_83__boxed_2638_; uint8_t v_res_2639_; lean_object* v_r_2640_; 
v_x_83__boxed_2638_ = lean_unbox(v_x_2636_);
v_res_2639_ = lp_solar__verify_List_foldl___at___00SolarVerify_Smt_impliesChain_spec__0(v_x_83__boxed_2638_, v_x_2637_);
lean_dec(v_x_2637_);
v_r_2640_ = lean_box(v_res_2639_);
return v_r_2640_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_impliesChain(lean_object* v_bs_2641_){
_start:
{
lean_object* v___x_2642_; 
v___x_2642_ = l_List_reverse___redArg(v_bs_2641_);
if (lean_obj_tag(v___x_2642_) == 1)
{
lean_object* v_tail_2643_; 
v_tail_2643_ = lean_ctor_get(v___x_2642_, 1);
lean_inc(v_tail_2643_);
if (lean_obj_tag(v_tail_2643_) == 1)
{
lean_object* v_head_2644_; uint8_t v___x_2645_; uint8_t v___x_2646_; lean_object* v___x_2647_; lean_object* v___x_2648_; 
v_head_2644_ = lean_ctor_get(v___x_2642_, 0);
lean_inc(v_head_2644_);
lean_dec_ref_known(v___x_2642_, 2);
v___x_2645_ = lean_unbox(v_head_2644_);
lean_dec(v_head_2644_);
v___x_2646_ = lp_solar__verify_List_foldl___at___00SolarVerify_Smt_impliesChain_spec__0(v___x_2645_, v_tail_2643_);
lean_dec_ref_known(v_tail_2643_, 2);
v___x_2647_ = lean_box(v___x_2646_);
v___x_2648_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v___x_2648_, 0, v___x_2647_);
return v___x_2648_;
}
else
{
lean_object* v___x_2649_; 
lean_dec_ref_known(v___x_2642_, 2);
lean_dec(v_tail_2643_);
v___x_2649_ = lean_box(0);
return v___x_2649_;
}
}
else
{
lean_object* v___x_2650_; 
lean_dec(v___x_2642_);
v___x_2650_ = lean_box(0);
return v___x_2650_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_intRel(lean_object* v_r_2651_, lean_object* v_args_2652_){
_start:
{
lean_object* v___x_2653_; 
v___x_2653_ = lp_solar__verify_SolarVerify_Smt_ints(v_args_2652_);
if (lean_obj_tag(v___x_2653_) == 0)
{
lean_object* v___x_2654_; 
lean_dec_ref(v_r_2651_);
v___x_2654_ = lean_box(0);
return v___x_2654_;
}
else
{
lean_object* v_val_2655_; lean_object* v___x_2657_; uint8_t v_isShared_2658_; uint8_t v_isSharedCheck_2677_; 
v_val_2655_ = lean_ctor_get(v___x_2653_, 0);
v_isSharedCheck_2677_ = !lean_is_exclusive(v___x_2653_);
if (v_isSharedCheck_2677_ == 0)
{
v___x_2657_ = v___x_2653_;
v_isShared_2658_ = v_isSharedCheck_2677_;
goto v_resetjp_2656_;
}
else
{
lean_inc(v_val_2655_);
lean_dec(v___x_2653_);
v___x_2657_ = lean_box(0);
v_isShared_2658_ = v_isSharedCheck_2677_;
goto v_resetjp_2656_;
}
v_resetjp_2656_:
{
if (lean_obj_tag(v_val_2655_) == 1)
{
lean_object* v_tail_2659_; 
v_tail_2659_ = lean_ctor_get(v_val_2655_, 1);
lean_inc(v_tail_2659_);
if (lean_obj_tag(v_tail_2659_) == 1)
{
lean_object* v_head_2660_; lean_object* v___x_2662_; uint8_t v_isShared_2663_; uint8_t v_isSharedCheck_2673_; 
v_head_2660_ = lean_ctor_get(v_val_2655_, 0);
v_isSharedCheck_2673_ = !lean_is_exclusive(v_val_2655_);
if (v_isSharedCheck_2673_ == 0)
{
lean_object* v_unused_2674_; 
v_unused_2674_ = lean_ctor_get(v_val_2655_, 1);
lean_dec(v_unused_2674_);
v___x_2662_ = v_val_2655_;
v_isShared_2663_ = v_isSharedCheck_2673_;
goto v_resetjp_2661_;
}
else
{
lean_inc(v_head_2660_);
lean_dec(v_val_2655_);
v___x_2662_ = lean_box(0);
v_isShared_2663_ = v_isSharedCheck_2673_;
goto v_resetjp_2661_;
}
v_resetjp_2661_:
{
lean_object* v___x_2664_; uint8_t v___x_2665_; lean_object* v___x_2666_; lean_object* v___x_2668_; 
v___x_2664_ = lean_box(0);
v___x_2665_ = lp_solar__verify_SolarVerify_Smt_chain___redArg(v_r_2651_, v_head_2660_, v_tail_2659_);
v___x_2666_ = lean_box(v___x_2665_);
if (v_isShared_2663_ == 0)
{
lean_ctor_set_tag(v___x_2662_, 0);
lean_ctor_set(v___x_2662_, 1, v___x_2666_);
lean_ctor_set(v___x_2662_, 0, v___x_2664_);
v___x_2668_ = v___x_2662_;
goto v_reusejp_2667_;
}
else
{
lean_object* v_reuseFailAlloc_2672_; 
v_reuseFailAlloc_2672_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2672_, 0, v___x_2664_);
lean_ctor_set(v_reuseFailAlloc_2672_, 1, v___x_2666_);
v___x_2668_ = v_reuseFailAlloc_2672_;
goto v_reusejp_2667_;
}
v_reusejp_2667_:
{
lean_object* v___x_2670_; 
if (v_isShared_2658_ == 0)
{
lean_ctor_set(v___x_2657_, 0, v___x_2668_);
v___x_2670_ = v___x_2657_;
goto v_reusejp_2669_;
}
else
{
lean_object* v_reuseFailAlloc_2671_; 
v_reuseFailAlloc_2671_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2671_, 0, v___x_2668_);
v___x_2670_ = v_reuseFailAlloc_2671_;
goto v_reusejp_2669_;
}
v_reusejp_2669_:
{
return v___x_2670_;
}
}
}
}
else
{
lean_object* v___x_2675_; 
lean_dec(v_tail_2659_);
lean_dec_ref_known(v_val_2655_, 2);
lean_del_object(v___x_2657_);
lean_dec_ref(v_r_2651_);
v___x_2675_ = lean_box(0);
return v___x_2675_;
}
}
else
{
lean_object* v___x_2676_; 
lean_del_object(v___x_2657_);
lean_dec(v_val_2655_);
lean_dec_ref(v_r_2651_);
v___x_2676_ = lean_box(0);
return v___x_2676_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bvRel(lean_object* v_r_2678_, lean_object* v_args_2679_){
_start:
{
lean_object* v___x_2680_; 
v___x_2680_ = lp_solar__verify_SolarVerify_Smt_bvs(v_args_2679_);
if (lean_obj_tag(v___x_2680_) == 0)
{
lean_object* v___x_2681_; 
lean_dec_ref(v_r_2678_);
v___x_2681_ = lean_box(0);
return v___x_2681_;
}
else
{
lean_object* v_val_2682_; lean_object* v___x_2684_; uint8_t v_isShared_2685_; uint8_t v_isSharedCheck_2708_; 
v_val_2682_ = lean_ctor_get(v___x_2680_, 0);
v_isSharedCheck_2708_ = !lean_is_exclusive(v___x_2680_);
if (v_isSharedCheck_2708_ == 0)
{
v___x_2684_ = v___x_2680_;
v_isShared_2685_ = v_isSharedCheck_2708_;
goto v_resetjp_2683_;
}
else
{
lean_inc(v_val_2682_);
lean_dec(v___x_2680_);
v___x_2684_ = lean_box(0);
v_isShared_2685_ = v_isSharedCheck_2708_;
goto v_resetjp_2683_;
}
v_resetjp_2683_:
{
lean_object* v_snd_2686_; 
v_snd_2686_ = lean_ctor_get(v_val_2682_, 1);
lean_inc(v_snd_2686_);
if (lean_obj_tag(v_snd_2686_) == 1)
{
lean_object* v_tail_2687_; 
v_tail_2687_ = lean_ctor_get(v_snd_2686_, 1);
lean_inc(v_tail_2687_);
if (lean_obj_tag(v_tail_2687_) == 1)
{
lean_object* v_tail_2688_; 
v_tail_2688_ = lean_ctor_get(v_tail_2687_, 1);
if (lean_obj_tag(v_tail_2688_) == 0)
{
lean_object* v_fst_2689_; lean_object* v___x_2691_; uint8_t v_isShared_2692_; uint8_t v_isSharedCheck_2703_; 
v_fst_2689_ = lean_ctor_get(v_val_2682_, 0);
v_isSharedCheck_2703_ = !lean_is_exclusive(v_val_2682_);
if (v_isSharedCheck_2703_ == 0)
{
lean_object* v_unused_2704_; 
v_unused_2704_ = lean_ctor_get(v_val_2682_, 1);
lean_dec(v_unused_2704_);
v___x_2691_ = v_val_2682_;
v_isShared_2692_ = v_isSharedCheck_2703_;
goto v_resetjp_2690_;
}
else
{
lean_inc(v_fst_2689_);
lean_dec(v_val_2682_);
v___x_2691_ = lean_box(0);
v_isShared_2692_ = v_isSharedCheck_2703_;
goto v_resetjp_2690_;
}
v_resetjp_2690_:
{
lean_object* v_head_2693_; lean_object* v_head_2694_; lean_object* v___x_2695_; lean_object* v___x_2696_; lean_object* v___x_2698_; 
v_head_2693_ = lean_ctor_get(v_snd_2686_, 0);
lean_inc(v_head_2693_);
lean_dec_ref_known(v_snd_2686_, 2);
v_head_2694_ = lean_ctor_get(v_tail_2687_, 0);
lean_inc(v_head_2694_);
lean_dec_ref_known(v_tail_2687_, 2);
v___x_2695_ = lean_box(0);
v___x_2696_ = lean_apply_3(v_r_2678_, v_fst_2689_, v_head_2693_, v_head_2694_);
if (v_isShared_2692_ == 0)
{
lean_ctor_set(v___x_2691_, 1, v___x_2696_);
lean_ctor_set(v___x_2691_, 0, v___x_2695_);
v___x_2698_ = v___x_2691_;
goto v_reusejp_2697_;
}
else
{
lean_object* v_reuseFailAlloc_2702_; 
v_reuseFailAlloc_2702_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2702_, 0, v___x_2695_);
lean_ctor_set(v_reuseFailAlloc_2702_, 1, v___x_2696_);
v___x_2698_ = v_reuseFailAlloc_2702_;
goto v_reusejp_2697_;
}
v_reusejp_2697_:
{
lean_object* v___x_2700_; 
if (v_isShared_2685_ == 0)
{
lean_ctor_set(v___x_2684_, 0, v___x_2698_);
v___x_2700_ = v___x_2684_;
goto v_reusejp_2699_;
}
else
{
lean_object* v_reuseFailAlloc_2701_; 
v_reuseFailAlloc_2701_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2701_, 0, v___x_2698_);
v___x_2700_ = v_reuseFailAlloc_2701_;
goto v_reusejp_2699_;
}
v_reusejp_2699_:
{
return v___x_2700_;
}
}
}
}
else
{
lean_object* v___x_2705_; 
lean_dec_ref_known(v_tail_2687_, 2);
lean_dec_ref_known(v_snd_2686_, 2);
lean_del_object(v___x_2684_);
lean_dec(v_val_2682_);
lean_dec_ref(v_r_2678_);
v___x_2705_ = lean_box(0);
return v___x_2705_;
}
}
else
{
lean_object* v___x_2706_; 
lean_dec(v_tail_2687_);
lean_dec_ref_known(v_snd_2686_, 2);
lean_del_object(v___x_2684_);
lean_dec(v_val_2682_);
lean_dec_ref(v_r_2678_);
v___x_2706_ = lean_box(0);
return v___x_2706_;
}
}
else
{
lean_object* v___x_2707_; 
lean_dec(v_snd_2686_);
lean_del_object(v___x_2684_);
lean_dec(v_val_2682_);
lean_dec_ref(v_r_2678_);
v___x_2707_ = lean_box(0);
return v___x_2707_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bvFold(lean_object* v_f_2709_, lean_object* v_args_2710_){
_start:
{
lean_object* v___x_2711_; 
v___x_2711_ = lp_solar__verify_SolarVerify_Smt_bvs(v_args_2710_);
if (lean_obj_tag(v___x_2711_) == 0)
{
lean_object* v___x_2712_; 
lean_dec_ref(v_f_2709_);
v___x_2712_ = lean_box(0);
return v___x_2712_;
}
else
{
lean_object* v_val_2713_; lean_object* v___x_2715_; uint8_t v_isShared_2716_; uint8_t v_isSharedCheck_2740_; 
v_val_2713_ = lean_ctor_get(v___x_2711_, 0);
v_isSharedCheck_2740_ = !lean_is_exclusive(v___x_2711_);
if (v_isSharedCheck_2740_ == 0)
{
v___x_2715_ = v___x_2711_;
v_isShared_2716_ = v_isSharedCheck_2740_;
goto v_resetjp_2714_;
}
else
{
lean_inc(v_val_2713_);
lean_dec(v___x_2711_);
v___x_2715_ = lean_box(0);
v_isShared_2716_ = v_isSharedCheck_2740_;
goto v_resetjp_2714_;
}
v_resetjp_2714_:
{
lean_object* v_fst_2717_; lean_object* v_snd_2718_; lean_object* v___x_2720_; uint8_t v_isShared_2721_; uint8_t v_isSharedCheck_2739_; 
v_fst_2717_ = lean_ctor_get(v_val_2713_, 0);
v_snd_2718_ = lean_ctor_get(v_val_2713_, 1);
v_isSharedCheck_2739_ = !lean_is_exclusive(v_val_2713_);
if (v_isSharedCheck_2739_ == 0)
{
v___x_2720_ = v_val_2713_;
v_isShared_2721_ = v_isSharedCheck_2739_;
goto v_resetjp_2719_;
}
else
{
lean_inc(v_snd_2718_);
lean_inc(v_fst_2717_);
lean_dec(v_val_2713_);
v___x_2720_ = lean_box(0);
v_isShared_2721_ = v_isSharedCheck_2739_;
goto v_resetjp_2719_;
}
v_resetjp_2719_:
{
lean_object* v___x_2722_; lean_object* v___x_2723_; 
lean_inc(v_fst_2717_);
v___x_2722_ = lean_apply_1(v_f_2709_, v_fst_2717_);
v___x_2723_ = lp_solar__verify_SolarVerify_Smt_foldl2___redArg(v___x_2722_, v_snd_2718_);
if (lean_obj_tag(v___x_2723_) == 0)
{
lean_object* v___x_2724_; 
lean_del_object(v___x_2720_);
lean_dec(v_fst_2717_);
lean_del_object(v___x_2715_);
v___x_2724_ = lean_box(0);
return v___x_2724_;
}
else
{
lean_object* v_val_2725_; lean_object* v___x_2727_; uint8_t v_isShared_2728_; uint8_t v_isSharedCheck_2738_; 
v_val_2725_ = lean_ctor_get(v___x_2723_, 0);
v_isSharedCheck_2738_ = !lean_is_exclusive(v___x_2723_);
if (v_isSharedCheck_2738_ == 0)
{
v___x_2727_ = v___x_2723_;
v_isShared_2728_ = v_isSharedCheck_2738_;
goto v_resetjp_2726_;
}
else
{
lean_inc(v_val_2725_);
lean_dec(v___x_2723_);
v___x_2727_ = lean_box(0);
v_isShared_2728_ = v_isSharedCheck_2738_;
goto v_resetjp_2726_;
}
v_resetjp_2726_:
{
lean_object* v___x_2730_; 
if (v_isShared_2716_ == 0)
{
lean_ctor_set_tag(v___x_2715_, 2);
lean_ctor_set(v___x_2715_, 0, v_fst_2717_);
v___x_2730_ = v___x_2715_;
goto v_reusejp_2729_;
}
else
{
lean_object* v_reuseFailAlloc_2737_; 
v_reuseFailAlloc_2737_ = lean_alloc_ctor(2, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2737_, 0, v_fst_2717_);
v___x_2730_ = v_reuseFailAlloc_2737_;
goto v_reusejp_2729_;
}
v_reusejp_2729_:
{
lean_object* v___x_2732_; 
if (v_isShared_2721_ == 0)
{
lean_ctor_set(v___x_2720_, 1, v_val_2725_);
lean_ctor_set(v___x_2720_, 0, v___x_2730_);
v___x_2732_ = v___x_2720_;
goto v_reusejp_2731_;
}
else
{
lean_object* v_reuseFailAlloc_2736_; 
v_reuseFailAlloc_2736_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2736_, 0, v___x_2730_);
lean_ctor_set(v_reuseFailAlloc_2736_, 1, v_val_2725_);
v___x_2732_ = v_reuseFailAlloc_2736_;
goto v_reusejp_2731_;
}
v_reusejp_2731_:
{
lean_object* v___x_2734_; 
if (v_isShared_2728_ == 0)
{
lean_ctor_set(v___x_2727_, 0, v___x_2732_);
v___x_2734_ = v___x_2727_;
goto v_reusejp_2733_;
}
else
{
lean_object* v_reuseFailAlloc_2735_; 
v_reuseFailAlloc_2735_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2735_, 0, v___x_2732_);
v___x_2734_ = v_reuseFailAlloc_2735_;
goto v_reusejp_2733_;
}
v_reusejp_2733_:
{
return v___x_2734_;
}
}
}
}
}
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bvBin(lean_object* v_f_2741_, lean_object* v_args_2742_){
_start:
{
lean_object* v___x_2743_; 
v___x_2743_ = lp_solar__verify_SolarVerify_Smt_bvs(v_args_2742_);
if (lean_obj_tag(v___x_2743_) == 0)
{
lean_object* v___x_2744_; 
lean_dec_ref(v_f_2741_);
v___x_2744_ = lean_box(0);
return v___x_2744_;
}
else
{
lean_object* v_val_2745_; lean_object* v___x_2747_; uint8_t v_isShared_2748_; uint8_t v_isSharedCheck_2771_; 
v_val_2745_ = lean_ctor_get(v___x_2743_, 0);
v_isSharedCheck_2771_ = !lean_is_exclusive(v___x_2743_);
if (v_isSharedCheck_2771_ == 0)
{
v___x_2747_ = v___x_2743_;
v_isShared_2748_ = v_isSharedCheck_2771_;
goto v_resetjp_2746_;
}
else
{
lean_inc(v_val_2745_);
lean_dec(v___x_2743_);
v___x_2747_ = lean_box(0);
v_isShared_2748_ = v_isSharedCheck_2771_;
goto v_resetjp_2746_;
}
v_resetjp_2746_:
{
lean_object* v_snd_2749_; 
v_snd_2749_ = lean_ctor_get(v_val_2745_, 1);
lean_inc(v_snd_2749_);
if (lean_obj_tag(v_snd_2749_) == 1)
{
lean_object* v_tail_2750_; 
v_tail_2750_ = lean_ctor_get(v_snd_2749_, 1);
lean_inc(v_tail_2750_);
if (lean_obj_tag(v_tail_2750_) == 1)
{
lean_object* v_tail_2751_; 
v_tail_2751_ = lean_ctor_get(v_tail_2750_, 1);
if (lean_obj_tag(v_tail_2751_) == 0)
{
lean_object* v_fst_2752_; lean_object* v___x_2754_; uint8_t v_isShared_2755_; uint8_t v_isSharedCheck_2766_; 
v_fst_2752_ = lean_ctor_get(v_val_2745_, 0);
v_isSharedCheck_2766_ = !lean_is_exclusive(v_val_2745_);
if (v_isSharedCheck_2766_ == 0)
{
lean_object* v_unused_2767_; 
v_unused_2767_ = lean_ctor_get(v_val_2745_, 1);
lean_dec(v_unused_2767_);
v___x_2754_ = v_val_2745_;
v_isShared_2755_ = v_isSharedCheck_2766_;
goto v_resetjp_2753_;
}
else
{
lean_inc(v_fst_2752_);
lean_dec(v_val_2745_);
v___x_2754_ = lean_box(0);
v_isShared_2755_ = v_isSharedCheck_2766_;
goto v_resetjp_2753_;
}
v_resetjp_2753_:
{
lean_object* v_head_2756_; lean_object* v_head_2757_; lean_object* v___x_2758_; lean_object* v___x_2759_; lean_object* v___x_2761_; 
v_head_2756_ = lean_ctor_get(v_snd_2749_, 0);
lean_inc(v_head_2756_);
lean_dec_ref_known(v_snd_2749_, 2);
v_head_2757_ = lean_ctor_get(v_tail_2750_, 0);
lean_inc(v_head_2757_);
lean_dec_ref_known(v_tail_2750_, 2);
lean_inc(v_fst_2752_);
v___x_2758_ = lean_alloc_ctor(2, 1, 0);
lean_ctor_set(v___x_2758_, 0, v_fst_2752_);
v___x_2759_ = lean_apply_3(v_f_2741_, v_fst_2752_, v_head_2756_, v_head_2757_);
if (v_isShared_2755_ == 0)
{
lean_ctor_set(v___x_2754_, 1, v___x_2759_);
lean_ctor_set(v___x_2754_, 0, v___x_2758_);
v___x_2761_ = v___x_2754_;
goto v_reusejp_2760_;
}
else
{
lean_object* v_reuseFailAlloc_2765_; 
v_reuseFailAlloc_2765_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2765_, 0, v___x_2758_);
lean_ctor_set(v_reuseFailAlloc_2765_, 1, v___x_2759_);
v___x_2761_ = v_reuseFailAlloc_2765_;
goto v_reusejp_2760_;
}
v_reusejp_2760_:
{
lean_object* v___x_2763_; 
if (v_isShared_2748_ == 0)
{
lean_ctor_set(v___x_2747_, 0, v___x_2761_);
v___x_2763_ = v___x_2747_;
goto v_reusejp_2762_;
}
else
{
lean_object* v_reuseFailAlloc_2764_; 
v_reuseFailAlloc_2764_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2764_, 0, v___x_2761_);
v___x_2763_ = v_reuseFailAlloc_2764_;
goto v_reusejp_2762_;
}
v_reusejp_2762_:
{
return v___x_2763_;
}
}
}
}
else
{
lean_object* v___x_2768_; 
lean_dec_ref_known(v_tail_2750_, 2);
lean_dec_ref_known(v_snd_2749_, 2);
lean_del_object(v___x_2747_);
lean_dec(v_val_2745_);
lean_dec_ref(v_f_2741_);
v___x_2768_ = lean_box(0);
return v___x_2768_;
}
}
else
{
lean_object* v___x_2769_; 
lean_dec_ref_known(v_snd_2749_, 2);
lean_dec(v_tail_2750_);
lean_del_object(v___x_2747_);
lean_dec(v_val_2745_);
lean_dec_ref(v_f_2741_);
v___x_2769_ = lean_box(0);
return v___x_2769_;
}
}
else
{
lean_object* v___x_2770_; 
lean_dec(v_snd_2749_);
lean_del_object(v___x_2747_);
lean_dec(v_val_2745_);
lean_dec_ref(v_f_2741_);
v___x_2770_ = lean_box(0);
return v___x_2770_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bvUn(lean_object* v_f_2772_, lean_object* v_args_2773_){
_start:
{
lean_object* v___x_2774_; 
v___x_2774_ = lp_solar__verify_SolarVerify_Smt_bvs(v_args_2773_);
if (lean_obj_tag(v___x_2774_) == 0)
{
lean_object* v___x_2775_; 
lean_dec_ref(v_f_2772_);
v___x_2775_ = lean_box(0);
return v___x_2775_;
}
else
{
lean_object* v_val_2776_; lean_object* v___x_2778_; uint8_t v_isShared_2779_; uint8_t v_isSharedCheck_2799_; 
v_val_2776_ = lean_ctor_get(v___x_2774_, 0);
v_isSharedCheck_2799_ = !lean_is_exclusive(v___x_2774_);
if (v_isSharedCheck_2799_ == 0)
{
v___x_2778_ = v___x_2774_;
v_isShared_2779_ = v_isSharedCheck_2799_;
goto v_resetjp_2777_;
}
else
{
lean_inc(v_val_2776_);
lean_dec(v___x_2774_);
v___x_2778_ = lean_box(0);
v_isShared_2779_ = v_isSharedCheck_2799_;
goto v_resetjp_2777_;
}
v_resetjp_2777_:
{
lean_object* v_snd_2780_; 
v_snd_2780_ = lean_ctor_get(v_val_2776_, 1);
lean_inc(v_snd_2780_);
if (lean_obj_tag(v_snd_2780_) == 1)
{
lean_object* v_tail_2781_; 
v_tail_2781_ = lean_ctor_get(v_snd_2780_, 1);
if (lean_obj_tag(v_tail_2781_) == 0)
{
lean_object* v_fst_2782_; lean_object* v___x_2784_; uint8_t v_isShared_2785_; uint8_t v_isSharedCheck_2795_; 
v_fst_2782_ = lean_ctor_get(v_val_2776_, 0);
v_isSharedCheck_2795_ = !lean_is_exclusive(v_val_2776_);
if (v_isSharedCheck_2795_ == 0)
{
lean_object* v_unused_2796_; 
v_unused_2796_ = lean_ctor_get(v_val_2776_, 1);
lean_dec(v_unused_2796_);
v___x_2784_ = v_val_2776_;
v_isShared_2785_ = v_isSharedCheck_2795_;
goto v_resetjp_2783_;
}
else
{
lean_inc(v_fst_2782_);
lean_dec(v_val_2776_);
v___x_2784_ = lean_box(0);
v_isShared_2785_ = v_isSharedCheck_2795_;
goto v_resetjp_2783_;
}
v_resetjp_2783_:
{
lean_object* v_head_2786_; lean_object* v___x_2787_; lean_object* v___x_2788_; lean_object* v___x_2790_; 
v_head_2786_ = lean_ctor_get(v_snd_2780_, 0);
lean_inc(v_head_2786_);
lean_dec_ref_known(v_snd_2780_, 2);
lean_inc(v_fst_2782_);
v___x_2787_ = lean_alloc_ctor(2, 1, 0);
lean_ctor_set(v___x_2787_, 0, v_fst_2782_);
v___x_2788_ = lean_apply_2(v_f_2772_, v_fst_2782_, v_head_2786_);
if (v_isShared_2785_ == 0)
{
lean_ctor_set(v___x_2784_, 1, v___x_2788_);
lean_ctor_set(v___x_2784_, 0, v___x_2787_);
v___x_2790_ = v___x_2784_;
goto v_reusejp_2789_;
}
else
{
lean_object* v_reuseFailAlloc_2794_; 
v_reuseFailAlloc_2794_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2794_, 0, v___x_2787_);
lean_ctor_set(v_reuseFailAlloc_2794_, 1, v___x_2788_);
v___x_2790_ = v_reuseFailAlloc_2794_;
goto v_reusejp_2789_;
}
v_reusejp_2789_:
{
lean_object* v___x_2792_; 
if (v_isShared_2779_ == 0)
{
lean_ctor_set(v___x_2778_, 0, v___x_2790_);
v___x_2792_ = v___x_2778_;
goto v_reusejp_2791_;
}
else
{
lean_object* v_reuseFailAlloc_2793_; 
v_reuseFailAlloc_2793_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v_reuseFailAlloc_2793_, 0, v___x_2790_);
v___x_2792_ = v_reuseFailAlloc_2793_;
goto v_reusejp_2791_;
}
v_reusejp_2791_:
{
return v___x_2792_;
}
}
}
}
else
{
lean_object* v___x_2797_; 
lean_dec_ref_known(v_snd_2780_, 2);
lean_del_object(v___x_2778_);
lean_dec(v_val_2776_);
lean_dec_ref(v_f_2772_);
v___x_2797_ = lean_box(0);
return v___x_2797_;
}
}
else
{
lean_object* v___x_2798_; 
lean_dec(v_snd_2780_);
lean_del_object(v___x_2778_);
lean_dec(v_val_2776_);
lean_dec_ref(v_f_2772_);
v___x_2798_ = lean_box(0);
return v___x_2798_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_sortsOf(lean_object* v_x_2800_){
_start:
{
if (lean_obj_tag(v_x_2800_) == 0)
{
lean_object* v___x_2801_; 
v___x_2801_ = lean_box(0);
return v___x_2801_;
}
else
{
lean_object* v_head_2802_; lean_object* v_tail_2803_; lean_object* v___x_2805_; uint8_t v_isShared_2806_; uint8_t v_isSharedCheck_2812_; 
v_head_2802_ = lean_ctor_get(v_x_2800_, 0);
v_tail_2803_ = lean_ctor_get(v_x_2800_, 1);
v_isSharedCheck_2812_ = !lean_is_exclusive(v_x_2800_);
if (v_isSharedCheck_2812_ == 0)
{
v___x_2805_ = v_x_2800_;
v_isShared_2806_ = v_isSharedCheck_2812_;
goto v_resetjp_2804_;
}
else
{
lean_inc(v_tail_2803_);
lean_inc(v_head_2802_);
lean_dec(v_x_2800_);
v___x_2805_ = lean_box(0);
v_isShared_2806_ = v_isSharedCheck_2812_;
goto v_resetjp_2804_;
}
v_resetjp_2804_:
{
lean_object* v_fst_2807_; lean_object* v___x_2808_; lean_object* v___x_2810_; 
v_fst_2807_ = lean_ctor_get(v_head_2802_, 0);
lean_inc(v_fst_2807_);
lean_dec(v_head_2802_);
v___x_2808_ = lp_solar__verify_SolarVerify_Smt_sortsOf(v_tail_2803_);
if (v_isShared_2806_ == 0)
{
lean_ctor_set(v___x_2805_, 1, v___x_2808_);
lean_ctor_set(v___x_2805_, 0, v_fst_2807_);
v___x_2810_ = v___x_2805_;
goto v_reusejp_2809_;
}
else
{
lean_object* v_reuseFailAlloc_2811_; 
v_reuseFailAlloc_2811_ = lean_alloc_ctor(1, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2811_, 0, v_fst_2807_);
lean_ctor_set(v_reuseFailAlloc_2811_, 1, v___x_2808_);
v___x_2810_ = v_reuseFailAlloc_2811_;
goto v_reusejp_2809_;
}
v_reusejp_2809_:
{
return v___x_2810_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_pack(lean_object* v_x_2813_){
_start:
{
if (lean_obj_tag(v_x_2813_) == 0)
{
lean_object* v___x_2814_; 
v___x_2814_ = lean_box(0);
return v___x_2814_;
}
else
{
lean_object* v_head_2815_; lean_object* v_tail_2816_; lean_object* v_snd_2817_; lean_object* v___x_2819_; uint8_t v_isShared_2820_; uint8_t v_isSharedCheck_2825_; 
v_head_2815_ = lean_ctor_get(v_x_2813_, 0);
lean_inc(v_head_2815_);
v_tail_2816_ = lean_ctor_get(v_x_2813_, 1);
lean_inc(v_tail_2816_);
lean_dec_ref_known(v_x_2813_, 2);
v_snd_2817_ = lean_ctor_get(v_head_2815_, 1);
v_isSharedCheck_2825_ = !lean_is_exclusive(v_head_2815_);
if (v_isSharedCheck_2825_ == 0)
{
lean_object* v_unused_2826_; 
v_unused_2826_ = lean_ctor_get(v_head_2815_, 0);
lean_dec(v_unused_2826_);
v___x_2819_ = v_head_2815_;
v_isShared_2820_ = v_isSharedCheck_2825_;
goto v_resetjp_2818_;
}
else
{
lean_inc(v_snd_2817_);
lean_dec(v_head_2815_);
v___x_2819_ = lean_box(0);
v_isShared_2820_ = v_isSharedCheck_2825_;
goto v_resetjp_2818_;
}
v_resetjp_2818_:
{
lean_object* v___x_2821_; lean_object* v___x_2823_; 
v___x_2821_ = lp_solar__verify_SolarVerify_Smt_pack(v_tail_2816_);
if (v_isShared_2820_ == 0)
{
lean_ctor_set(v___x_2819_, 1, v___x_2821_);
lean_ctor_set(v___x_2819_, 0, v_snd_2817_);
v___x_2823_ = v___x_2819_;
goto v_reusejp_2822_;
}
else
{
lean_object* v_reuseFailAlloc_2824_; 
v_reuseFailAlloc_2824_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_2824_, 0, v_snd_2817_);
lean_ctor_set(v_reuseFailAlloc_2824_, 1, v___x_2821_);
v___x_2823_ = v_reuseFailAlloc_2824_;
goto v_reusejp_2822_;
}
v_reusejp_2822_:
{
return v___x_2823_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Smt_bindAll(lean_object* v_ss_2827_, lean_object* v_xs_2828_, lean_object* v_locals_2829_){
_start:
{
lean_object* v___x_2830_; lean_object* v___x_2831_; lean_object* v___x_2832_; 
v___x_2830_ = lp_solar__verify_SolarVerify_Smt_Srt_unpack(v_ss_2827_, v_xs_2828_);
v___x_2831_ = l_List_reverse___redArg(v___x_2830_);
v___x_2832_ = l_List_appendTR___redArg(v___x_2831_, v_locals_2829_);
return v___x_2832_;
}
}
lean_object* initialize_Init(uint8_t builtin);
lean_object* initialize_Init(uint8_t builtin);
void lean_initialize_runtime_module();
static bool _G_initialized = false;
LEAN_EXPORT lean_object* initialize_solar__verify_SolarVerify_Smt(uint8_t builtin) {
lean_object * res;
if (_G_initialized) return lean_io_result_mk_ok(lean_box(0));
_G_initialized = true;
lean_initialize_runtime_module();
res = initialize_Init(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
res = initialize_Init(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
lp_solar__verify_SolarVerify_Smt_instInhabitedSrt_default = _init_lp_solar__verify_SolarVerify_Smt_instInhabitedSrt_default();
lean_mark_persistent(lp_solar__verify_SolarVerify_Smt_instInhabitedSrt_default);
lp_solar__verify_SolarVerify_Smt_instInhabitedSrt = _init_lp_solar__verify_SolarVerify_Smt_instInhabitedSrt();
lean_mark_persistent(lp_solar__verify_SolarVerify_Smt_instInhabitedSrt);
lp_solar__verify_SolarVerify_Smt_instInhabitedOp_default = _init_lp_solar__verify_SolarVerify_Smt_instInhabitedOp_default();
lean_mark_persistent(lp_solar__verify_SolarVerify_Smt_instInhabitedOp_default);
lp_solar__verify_SolarVerify_Smt_instInhabitedOp = _init_lp_solar__verify_SolarVerify_Smt_instInhabitedOp();
lean_mark_persistent(lp_solar__verify_SolarVerify_Smt_instInhabitedOp);
return lean_io_result_mk_ok(lean_box(0));
}
#ifdef __cplusplus
}
#endif
