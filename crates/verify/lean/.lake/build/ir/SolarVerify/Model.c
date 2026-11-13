// Lean compiler output
// Module: SolarVerify.Model
// Imports: public import Init public meta import Init public import SolarVerify.Smt
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
lean_object* lp_solar__verify_SolarVerify_Smt_Val_as(lean_object*, lean_object*);
uint8_t lean_int_dec_eq(lean_object*, lean_object*);
uint8_t lean_nat_dec_eq(lean_object*, lean_object*);
lean_object* lp_solar__verify_SolarVerify_Smt_Srt_default(lean_object*);
lean_object* l_List_get_x3fInternal___redArg(lean_object*, lean_object*);
lean_object* lp_solar__verify_SolarVerify_Smt_Srt_unpack(lean_object*, lean_object*);
lean_object* lean_nat_to_int(lean_object*);
lean_object* l_List_lengthTR___redArg(lean_object*);
lean_object* l_List_zipWith___at___00List_zip_spec__0___redArg(lean_object*, lean_object*);
lean_object* lp_solar__verify_SolarVerify_Smt_sortsOf(lean_object*);
lean_object* lp_solar__verify_SolarVerify_Smt_pack(lean_object*);
lean_object* l_BitVec_ofNat(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_int(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_bool(uint8_t);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_bool___boxed(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_bv(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_bv___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_tup(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_valAt(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_valAt___boxed(lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Model_same_firstOrderEq(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Model_same_tupEq(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_same_tupEq___boxed(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_same_firstOrderEq___boxed(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Model_same(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_same___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_find_x3f___at___00SolarVerify_Model_array_spec__0(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_array___lam__0(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_array___lam__0___boxed(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_array(lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT uint8_t lp_solar__verify_List_all___at___00SolarVerify_Model_table_spec__0(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_all___at___00SolarVerify_Model_table_spec__0___boxed(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_List_find_x3f___at___00SolarVerify_Model_table_spec__1(lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_table(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_table___boxed(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_env___lam__0(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_env___lam__0___boxed(lean_object*, lean_object*, lean_object*, lean_object*, lean_object*);
static lean_once_cell_t lp_solar__verify_SolarVerify_Model_env___lam__1___closed__0_once = LEAN_ONCE_CELL_INITIALIZER;
static lean_object* lp_solar__verify_SolarVerify_Model_env___lam__1___closed__0;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_env___lam__1(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_env___lam__1___boxed(lean_object*);
static const lean_closure_object lp_solar__verify_SolarVerify_Model_env___closed__0_value = {.m_header = {.m_rc = 0, .m_cs_sz = sizeof(lean_closure_object) + sizeof(void*)*0, .m_other = 0, .m_tag = 245}, .m_fun = (void*)lp_solar__verify_SolarVerify_Model_env___lam__1___boxed, .m_arity = 1, .m_num_fixed = 0, .m_objs = {} };
static const lean_object* lp_solar__verify_SolarVerify_Model_env___closed__0 = (const lean_object*)&lp_solar__verify_SolarVerify_Model_env___closed__0_value;
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_env(lean_object*);
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_int(lean_object* v_i_1_){
_start:
{
lean_object* v___x_2_; lean_object* v___x_3_; 
v___x_2_ = lean_box(1);
v___x_3_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v___x_3_, 0, v___x_2_);
lean_ctor_set(v___x_3_, 1, v_i_1_);
return v___x_3_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_bool(uint8_t v_b_4_){
_start:
{
lean_object* v___x_5_; lean_object* v___x_6_; lean_object* v___x_7_; 
v___x_5_ = lean_box(0);
v___x_6_ = lean_box(v_b_4_);
v___x_7_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v___x_7_, 0, v___x_5_);
lean_ctor_set(v___x_7_, 1, v___x_6_);
return v___x_7_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_bool___boxed(lean_object* v_b_8_){
_start:
{
uint8_t v_b_boxed_9_; lean_object* v_res_10_; 
v_b_boxed_9_ = lean_unbox(v_b_8_);
v_res_10_ = lp_solar__verify_SolarVerify_Model_bool(v_b_boxed_9_);
return v_res_10_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_bv(lean_object* v_w_11_, lean_object* v_n_12_){
_start:
{
lean_object* v___x_13_; lean_object* v_this_14_; lean_object* v___x_15_; 
lean_inc(v_w_11_);
v___x_13_ = lean_alloc_ctor(2, 1, 0);
lean_ctor_set(v___x_13_, 0, v_w_11_);
v_this_14_ = l_BitVec_ofNat(v_w_11_, v_n_12_);
lean_dec(v_w_11_);
v___x_15_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v___x_15_, 0, v___x_13_);
lean_ctor_set(v___x_15_, 1, v_this_14_);
return v___x_15_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_bv___boxed(lean_object* v_w_16_, lean_object* v_n_17_){
_start:
{
lean_object* v_res_18_; 
v_res_18_ = lp_solar__verify_SolarVerify_Model_bv(v_w_16_, v_n_17_);
lean_dec(v_n_17_);
return v_res_18_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_tup(lean_object* v_fields_19_){
_start:
{
lean_object* v___x_20_; lean_object* v___x_21_; lean_object* v___x_22_; lean_object* v___x_23_; 
lean_inc(v_fields_19_);
v___x_20_ = lp_solar__verify_SolarVerify_Smt_sortsOf(v_fields_19_);
v___x_21_ = lean_alloc_ctor(4, 1, 0);
lean_ctor_set(v___x_21_, 0, v___x_20_);
v___x_22_ = lp_solar__verify_SolarVerify_Smt_pack(v_fields_19_);
v___x_23_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v___x_23_, 0, v___x_21_);
lean_ctor_set(v___x_23_, 1, v___x_22_);
return v___x_23_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_valAt(lean_object* v_s_24_, lean_object* v_v_25_){
_start:
{
lean_object* v___x_26_; 
v___x_26_ = lp_solar__verify_SolarVerify_Smt_Val_as(v_v_25_, v_s_24_);
if (lean_obj_tag(v___x_26_) == 0)
{
lean_object* v___x_27_; 
v___x_27_ = lp_solar__verify_SolarVerify_Smt_Srt_default(v_s_24_);
return v___x_27_;
}
else
{
lean_object* v_val_28_; 
lean_dec(v_s_24_);
v_val_28_ = lean_ctor_get(v___x_26_, 0);
lean_inc(v_val_28_);
lean_dec_ref_known(v___x_26_, 1);
return v_val_28_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_valAt___boxed(lean_object* v_s_29_, lean_object* v_v_30_){
_start:
{
lean_object* v_res_31_; 
v_res_31_ = lp_solar__verify_SolarVerify_Model_valAt(v_s_29_, v_v_30_);
lean_dec_ref(v_v_30_);
return v_res_31_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Model_same_firstOrderEq(lean_object* v_s_32_, lean_object* v_a_33_, lean_object* v_a_34_){
_start:
{
switch(lean_obj_tag(v_s_32_))
{
case 0:
{
uint8_t v___x_35_; 
v___x_35_ = lean_unbox(v_a_34_);
if (v___x_35_ == 0)
{
uint8_t v___x_36_; 
v___x_36_ = lean_unbox(v_a_33_);
if (v___x_36_ == 0)
{
uint8_t v___x_37_; 
v___x_37_ = 1;
return v___x_37_;
}
else
{
uint8_t v___x_38_; 
v___x_38_ = lean_unbox(v_a_34_);
return v___x_38_;
}
}
else
{
uint8_t v___x_39_; 
v___x_39_ = lean_unbox(v_a_33_);
return v___x_39_;
}
}
case 1:
{
uint8_t v___x_40_; 
v___x_40_ = lean_int_dec_eq(v_a_33_, v_a_34_);
return v___x_40_;
}
case 2:
{
uint8_t v___x_41_; 
v___x_41_ = lean_nat_dec_eq(v_a_33_, v_a_34_);
return v___x_41_;
}
case 3:
{
uint8_t v___x_42_; 
v___x_42_ = 0;
return v___x_42_;
}
default: 
{
lean_object* v_fields_43_; uint8_t v___x_44_; 
v_fields_43_ = lean_ctor_get(v_s_32_, 0);
v___x_44_ = lp_solar__verify_SolarVerify_Model_same_tupEq(v_fields_43_, v_a_33_, v_a_34_);
return v___x_44_;
}
}
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Model_same_tupEq(lean_object* v_ss_45_, lean_object* v_a_46_, lean_object* v_a_47_){
_start:
{
if (lean_obj_tag(v_ss_45_) == 0)
{
uint8_t v___x_48_; 
v___x_48_ = 1;
return v___x_48_;
}
else
{
lean_object* v_head_49_; lean_object* v_tail_50_; lean_object* v_fst_51_; lean_object* v_snd_52_; lean_object* v_fst_53_; lean_object* v_snd_54_; uint8_t v___x_55_; 
v_head_49_ = lean_ctor_get(v_ss_45_, 0);
v_tail_50_ = lean_ctor_get(v_ss_45_, 1);
v_fst_51_ = lean_ctor_get(v_a_46_, 0);
v_snd_52_ = lean_ctor_get(v_a_46_, 1);
v_fst_53_ = lean_ctor_get(v_a_47_, 0);
v_snd_54_ = lean_ctor_get(v_a_47_, 1);
v___x_55_ = lp_solar__verify_SolarVerify_Model_same_firstOrderEq(v_head_49_, v_fst_51_, v_fst_53_);
if (v___x_55_ == 0)
{
return v___x_55_;
}
else
{
v_ss_45_ = v_tail_50_;
v_a_46_ = v_snd_52_;
v_a_47_ = v_snd_54_;
goto _start;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_same_tupEq___boxed(lean_object* v_ss_57_, lean_object* v_a_58_, lean_object* v_a_59_){
_start:
{
uint8_t v_res_60_; lean_object* v_r_61_; 
v_res_60_ = lp_solar__verify_SolarVerify_Model_same_tupEq(v_ss_57_, v_a_58_, v_a_59_);
lean_dec(v_a_59_);
lean_dec(v_a_58_);
lean_dec(v_ss_57_);
v_r_61_ = lean_box(v_res_60_);
return v_r_61_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_same_firstOrderEq___boxed(lean_object* v_s_62_, lean_object* v_a_63_, lean_object* v_a_64_){
_start:
{
uint8_t v_res_65_; lean_object* v_r_66_; 
v_res_65_ = lp_solar__verify_SolarVerify_Model_same_firstOrderEq(v_s_62_, v_a_63_, v_a_64_);
lean_dec(v_a_64_);
lean_dec(v_a_63_);
lean_dec(v_s_62_);
v_r_66_ = lean_box(v_res_65_);
return v_r_66_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_SolarVerify_Model_same(lean_object* v_a_67_, lean_object* v_b_68_){
_start:
{
lean_object* v_fst_69_; lean_object* v_snd_70_; lean_object* v___x_71_; 
v_fst_69_ = lean_ctor_get(v_a_67_, 0);
v_snd_70_ = lean_ctor_get(v_a_67_, 1);
v___x_71_ = lp_solar__verify_SolarVerify_Smt_Val_as(v_b_68_, v_fst_69_);
if (lean_obj_tag(v___x_71_) == 0)
{
uint8_t v___x_72_; 
v___x_72_ = 0;
return v___x_72_;
}
else
{
if (lean_obj_tag(v_fst_69_) == 3)
{
uint8_t v___x_73_; 
lean_dec_ref_known(v___x_71_, 1);
v___x_73_ = 0;
return v___x_73_;
}
else
{
lean_object* v_val_74_; uint8_t v___x_75_; 
v_val_74_ = lean_ctor_get(v___x_71_, 0);
lean_inc(v_val_74_);
lean_dec_ref_known(v___x_71_, 1);
v___x_75_ = lp_solar__verify_SolarVerify_Model_same_firstOrderEq(v_fst_69_, v_snd_70_, v_val_74_);
lean_dec(v_val_74_);
return v___x_75_;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_same___boxed(lean_object* v_a_76_, lean_object* v_b_77_){
_start:
{
uint8_t v_res_78_; lean_object* v_r_79_; 
v_res_78_ = lp_solar__verify_SolarVerify_Model_same(v_a_76_, v_b_77_);
lean_dec_ref(v_b_77_);
lean_dec_ref(v_a_76_);
v_r_79_ = lean_box(v_res_78_);
return v_r_79_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_find_x3f___at___00SolarVerify_Model_array_spec__0(lean_object* v_idx_80_, lean_object* v_k_81_, lean_object* v_x_82_){
_start:
{
if (lean_obj_tag(v_x_82_) == 0)
{
lean_object* v___x_83_; 
lean_dec(v_k_81_);
lean_dec(v_idx_80_);
v___x_83_ = lean_box(0);
return v___x_83_;
}
else
{
lean_object* v_head_84_; lean_object* v_tail_85_; lean_object* v___x_87_; uint8_t v_isShared_88_; uint8_t v_isSharedCheck_96_; 
v_head_84_ = lean_ctor_get(v_x_82_, 0);
v_tail_85_ = lean_ctor_get(v_x_82_, 1);
v_isSharedCheck_96_ = !lean_is_exclusive(v_x_82_);
if (v_isSharedCheck_96_ == 0)
{
v___x_87_ = v_x_82_;
v_isShared_88_ = v_isSharedCheck_96_;
goto v_resetjp_86_;
}
else
{
lean_inc(v_tail_85_);
lean_inc(v_head_84_);
lean_dec(v_x_82_);
v___x_87_ = lean_box(0);
v_isShared_88_ = v_isSharedCheck_96_;
goto v_resetjp_86_;
}
v_resetjp_86_:
{
lean_object* v_fst_89_; lean_object* v___x_91_; 
v_fst_89_ = lean_ctor_get(v_head_84_, 0);
lean_inc(v_k_81_);
lean_inc(v_idx_80_);
if (v_isShared_88_ == 0)
{
lean_ctor_set_tag(v___x_87_, 0);
lean_ctor_set(v___x_87_, 1, v_k_81_);
lean_ctor_set(v___x_87_, 0, v_idx_80_);
v___x_91_ = v___x_87_;
goto v_reusejp_90_;
}
else
{
lean_object* v_reuseFailAlloc_95_; 
v_reuseFailAlloc_95_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v_reuseFailAlloc_95_, 0, v_idx_80_);
lean_ctor_set(v_reuseFailAlloc_95_, 1, v_k_81_);
v___x_91_ = v_reuseFailAlloc_95_;
goto v_reusejp_90_;
}
v_reusejp_90_:
{
uint8_t v___x_92_; 
v___x_92_ = lp_solar__verify_SolarVerify_Model_same(v_fst_89_, v___x_91_);
lean_dec_ref(v___x_91_);
if (v___x_92_ == 0)
{
lean_dec(v_head_84_);
v_x_82_ = v_tail_85_;
goto _start;
}
else
{
lean_object* v___x_94_; 
lean_dec(v_tail_85_);
lean_dec(v_k_81_);
lean_dec(v_idx_80_);
v___x_94_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v___x_94_, 0, v_head_84_);
return v___x_94_;
}
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_array___lam__0(lean_object* v_idx_97_, lean_object* v_entries_98_, lean_object* v_elem_99_, lean_object* v_d_100_, lean_object* v_k_101_){
_start:
{
lean_object* v___x_102_; 
v___x_102_ = lp_solar__verify_List_find_x3f___at___00SolarVerify_Model_array_spec__0(v_idx_97_, v_k_101_, v_entries_98_);
if (lean_obj_tag(v___x_102_) == 0)
{
lean_object* v___x_103_; 
v___x_103_ = lp_solar__verify_SolarVerify_Model_valAt(v_elem_99_, v_d_100_);
return v___x_103_;
}
else
{
lean_object* v_val_104_; lean_object* v_snd_105_; lean_object* v___x_106_; 
v_val_104_ = lean_ctor_get(v___x_102_, 0);
lean_inc(v_val_104_);
lean_dec_ref_known(v___x_102_, 1);
v_snd_105_ = lean_ctor_get(v_val_104_, 1);
lean_inc(v_snd_105_);
lean_dec(v_val_104_);
v___x_106_ = lp_solar__verify_SolarVerify_Model_valAt(v_elem_99_, v_snd_105_);
lean_dec(v_snd_105_);
return v___x_106_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_array___lam__0___boxed(lean_object* v_idx_107_, lean_object* v_entries_108_, lean_object* v_elem_109_, lean_object* v_d_110_, lean_object* v_k_111_){
_start:
{
lean_object* v_res_112_; 
v_res_112_ = lp_solar__verify_SolarVerify_Model_array___lam__0(v_idx_107_, v_entries_108_, v_elem_109_, v_d_110_, v_k_111_);
lean_dec_ref(v_d_110_);
return v_res_112_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_array(lean_object* v_idx_113_, lean_object* v_elem_114_, lean_object* v_d_115_, lean_object* v_entries_116_){
_start:
{
lean_object* v___f_117_; lean_object* v___x_118_; lean_object* v___x_119_; 
lean_inc(v_elem_114_);
lean_inc(v_idx_113_);
v___f_117_ = lean_alloc_closure((void*)(lp_solar__verify_SolarVerify_Model_array___lam__0___boxed), 5, 4);
lean_closure_set(v___f_117_, 0, v_idx_113_);
lean_closure_set(v___f_117_, 1, v_entries_116_);
lean_closure_set(v___f_117_, 2, v_elem_114_);
lean_closure_set(v___f_117_, 3, v_d_115_);
v___x_118_ = lean_alloc_ctor(3, 2, 0);
lean_ctor_set(v___x_118_, 0, v_idx_113_);
lean_ctor_set(v___x_118_, 1, v_elem_114_);
v___x_119_ = lean_alloc_ctor(0, 2, 0);
lean_ctor_set(v___x_119_, 0, v___x_118_);
lean_ctor_set(v___x_119_, 1, v___f_117_);
return v___x_119_;
}
}
LEAN_EXPORT uint8_t lp_solar__verify_List_all___at___00SolarVerify_Model_table_spec__0(lean_object* v_x_120_){
_start:
{
if (lean_obj_tag(v_x_120_) == 0)
{
uint8_t v___x_121_; 
v___x_121_ = 1;
return v___x_121_;
}
else
{
lean_object* v_head_122_; lean_object* v_tail_123_; lean_object* v_fst_124_; lean_object* v_snd_125_; uint8_t v___x_126_; 
v_head_122_ = lean_ctor_get(v_x_120_, 0);
v_tail_123_ = lean_ctor_get(v_x_120_, 1);
v_fst_124_ = lean_ctor_get(v_head_122_, 0);
v_snd_125_ = lean_ctor_get(v_head_122_, 1);
v___x_126_ = lp_solar__verify_SolarVerify_Model_same(v_fst_124_, v_snd_125_);
if (v___x_126_ == 0)
{
return v___x_126_;
}
else
{
v_x_120_ = v_tail_123_;
goto _start;
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_all___at___00SolarVerify_Model_table_spec__0___boxed(lean_object* v_x_128_){
_start:
{
uint8_t v_res_129_; lean_object* v_r_130_; 
v_res_129_ = lp_solar__verify_List_all___at___00SolarVerify_Model_table_spec__0(v_x_128_);
lean_dec(v_x_128_);
v_r_130_ = lean_box(v_res_129_);
return v_r_130_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_List_find_x3f___at___00SolarVerify_Model_table_spec__1(lean_object* v_args_131_, lean_object* v_x_132_){
_start:
{
if (lean_obj_tag(v_x_132_) == 0)
{
lean_object* v___x_133_; 
lean_dec(v_args_131_);
v___x_133_ = lean_box(0);
return v___x_133_;
}
else
{
lean_object* v_head_134_; lean_object* v_tail_135_; lean_object* v_fst_136_; lean_object* v___x_137_; lean_object* v___x_138_; uint8_t v___x_139_; 
v_head_134_ = lean_ctor_get(v_x_132_, 0);
lean_inc(v_head_134_);
v_tail_135_ = lean_ctor_get(v_x_132_, 1);
lean_inc(v_tail_135_);
lean_dec_ref_known(v_x_132_, 2);
v_fst_136_ = lean_ctor_get(v_head_134_, 0);
v___x_137_ = l_List_lengthTR___redArg(v_fst_136_);
v___x_138_ = l_List_lengthTR___redArg(v_args_131_);
v___x_139_ = lean_nat_dec_eq(v___x_137_, v___x_138_);
lean_dec(v___x_138_);
lean_dec(v___x_137_);
if (v___x_139_ == 0)
{
lean_dec(v_head_134_);
v_x_132_ = v_tail_135_;
goto _start;
}
else
{
lean_object* v___x_141_; uint8_t v___x_142_; 
lean_inc(v_args_131_);
lean_inc(v_fst_136_);
v___x_141_ = l_List_zipWith___at___00List_zip_spec__0___redArg(v_fst_136_, v_args_131_);
v___x_142_ = lp_solar__verify_List_all___at___00SolarVerify_Model_table_spec__0(v___x_141_);
lean_dec(v___x_141_);
if (v___x_142_ == 0)
{
lean_dec(v_head_134_);
v_x_132_ = v_tail_135_;
goto _start;
}
else
{
lean_object* v___x_144_; 
lean_dec(v_tail_135_);
lean_dec(v_args_131_);
v___x_144_ = lean_alloc_ctor(1, 1, 0);
lean_ctor_set(v___x_144_, 0, v_head_134_);
return v___x_144_;
}
}
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_table(lean_object* v_d_145_, lean_object* v_rows_146_, lean_object* v_args_147_){
_start:
{
lean_object* v___x_148_; 
v___x_148_ = lp_solar__verify_List_find_x3f___at___00SolarVerify_Model_table_spec__1(v_args_147_, v_rows_146_);
if (lean_obj_tag(v___x_148_) == 0)
{
lean_inc_ref(v_d_145_);
return v_d_145_;
}
else
{
lean_object* v_val_149_; lean_object* v_snd_150_; 
v_val_149_ = lean_ctor_get(v___x_148_, 0);
lean_inc(v_val_149_);
lean_dec_ref_known(v___x_148_, 1);
v_snd_150_ = lean_ctor_get(v_val_149_, 1);
lean_inc(v_snd_150_);
lean_dec(v_val_149_);
return v_snd_150_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_table___boxed(lean_object* v_d_151_, lean_object* v_rows_152_, lean_object* v_args_153_){
_start:
{
lean_object* v_res_154_; 
v_res_154_ = lp_solar__verify_SolarVerify_Model_table(v_d_151_, v_rows_152_, v_args_153_);
lean_dec_ref(v_d_151_);
return v_res_154_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_env___lam__0(lean_object* v_fns_155_, lean_object* v_f_156_, lean_object* v_args_157_, lean_object* v_ret_158_, lean_object* v_xs_159_){
_start:
{
lean_object* v___x_160_; 
v___x_160_ = l_List_get_x3fInternal___redArg(v_fns_155_, v_f_156_);
if (lean_obj_tag(v___x_160_) == 0)
{
lean_object* v___x_161_; 
lean_dec(v_xs_159_);
lean_dec(v_args_157_);
v___x_161_ = lp_solar__verify_SolarVerify_Smt_Srt_default(v_ret_158_);
return v___x_161_;
}
else
{
lean_object* v_val_162_; lean_object* v___x_163_; lean_object* v___x_164_; lean_object* v___x_165_; 
v_val_162_ = lean_ctor_get(v___x_160_, 0);
lean_inc(v_val_162_);
lean_dec_ref_known(v___x_160_, 1);
v___x_163_ = lp_solar__verify_SolarVerify_Smt_Srt_unpack(v_args_157_, v_xs_159_);
v___x_164_ = lean_apply_1(v_val_162_, v___x_163_);
v___x_165_ = lp_solar__verify_SolarVerify_Model_valAt(v_ret_158_, v___x_164_);
lean_dec_ref(v___x_164_);
return v___x_165_;
}
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_env___lam__0___boxed(lean_object* v_fns_166_, lean_object* v_f_167_, lean_object* v_args_168_, lean_object* v_ret_169_, lean_object* v_xs_170_){
_start:
{
lean_object* v_res_171_; 
v_res_171_ = lp_solar__verify_SolarVerify_Model_env___lam__0(v_fns_166_, v_f_167_, v_args_168_, v_ret_169_, v_xs_170_);
lean_dec(v_fns_166_);
return v_res_171_;
}
}
static lean_object* _init_lp_solar__verify_SolarVerify_Model_env___lam__1___closed__0(void){
_start:
{
lean_object* v___x_172_; lean_object* v___x_173_; 
v___x_172_ = lean_unsigned_to_nat(0u);
v___x_173_ = lean_nat_to_int(v___x_172_);
return v___x_173_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_env___lam__1(lean_object* v_x_174_){
_start:
{
lean_object* v___x_175_; 
v___x_175_ = lean_obj_once(&lp_solar__verify_SolarVerify_Model_env___lam__1___closed__0, &lp_solar__verify_SolarVerify_Model_env___lam__1___closed__0_once, _init_lp_solar__verify_SolarVerify_Model_env___lam__1___closed__0);
return v___x_175_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_env___lam__1___boxed(lean_object* v_x_176_){
_start:
{
lean_object* v_res_177_; 
v_res_177_ = lp_solar__verify_SolarVerify_Model_env___lam__1(v_x_176_);
lean_dec(v_x_176_);
return v_res_177_;
}
}
LEAN_EXPORT lean_object* lp_solar__verify_SolarVerify_Model_env(lean_object* v_fns_179_){
_start:
{
lean_object* v___f_180_; lean_object* v___f_181_; lean_object* v___x_182_; 
v___f_180_ = lean_alloc_closure((void*)(lp_solar__verify_SolarVerify_Model_env___lam__0___boxed), 5, 1);
lean_closure_set(v___f_180_, 0, v_fns_179_);
v___f_181_ = ((lean_object*)(lp_solar__verify_SolarVerify_Model_env___closed__0));
v___x_182_ = lean_alloc_ctor(0, 3, 0);
lean_ctor_set(v___x_182_, 0, v___f_180_);
lean_ctor_set(v___x_182_, 1, v___f_181_);
lean_ctor_set(v___x_182_, 2, v___f_181_);
return v___x_182_;
}
}
lean_object* initialize_Init(uint8_t builtin);
lean_object* initialize_Init(uint8_t builtin);
lean_object* initialize_solar__verify_SolarVerify_Smt(uint8_t builtin);
void lean_initialize_runtime_module();
static bool _G_initialized = false;
LEAN_EXPORT lean_object* initialize_solar__verify_SolarVerify_Model(uint8_t builtin) {
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
res = initialize_solar__verify_SolarVerify_Smt(builtin);
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
return lean_io_result_mk_ok(lean_box(0));
}
#ifdef __cplusplus
}
#endif
