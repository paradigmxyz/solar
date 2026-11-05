//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cu8_0::t_retdirect_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cu8_0::t_param_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cu8_0::t_mem_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cu8_0::t_local_encode_signature => 87186003289801191604053496905270320844958310796545158597700757130929743847240
//@ run-call: Cu8_0::t_retdirect_encode_signature => 87186003289801191604053496905270320844958310796545158597700757130929743847240
//@ run-call: Cu8_0::t_param_encode_signature => 87186003289801191604053496905270320844958310796545158597700757130929743847240
//@ run-call: Cu8_0::t_mem_encode_signature => 87186003289801191604053496905270320844958310796545158597700757130929743847240
//@ run-call: Cu8_0::t_local_encode_call => 8531465725870555205293184461453974544980304869217186201431725384886034049810
//@ run-call: Cu8_0::t_retdirect_encode_call => 8531465725870555205293184461453974544980304869217186201431725384886034049810
//@ run-call: Cu8_0::t_param_encode_call => 8531465725870555205293184461453974544980304869217186201431725384886034049810
//@ run-call: Cu8_0::t_mem_encode_call => 8531465725870555205293184461453974544980304869217186201431725384886034049810
//@ run-call: Cu64_0::t_local_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cu64_0::t_retdirect_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cu64_0::t_param_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cu64_0::t_mem_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cu64_0::t_local_encode_signature => 7832669481540590511110982737699169732064686723106378716292462605095210590262
//@ run-call: Cu64_0::t_retdirect_encode_signature => 7832669481540590511110982737699169732064686723106378716292462605095210590262
//@ run-call: Cu64_0::t_param_encode_signature => 7832669481540590511110982737699169732064686723106378716292462605095210590262
//@ run-call: Cu64_0::t_mem_encode_signature => 7832669481540590511110982737699169732064686723106378716292462605095210590262
//@ run-call: Cu64_0::t_local_encode_call => 10913393400415515728234496752578584720374155542496613235070426267999009432008
//@ run-call: Cu64_0::t_retdirect_encode_call => 10913393400415515728234496752578584720374155542496613235070426267999009432008
//@ run-call: Cu64_0::t_param_encode_call => 10913393400415515728234496752578584720374155542496613235070426267999009432008
//@ run-call: Cu64_0::t_mem_encode_call => 10913393400415515728234496752578584720374155542496613235070426267999009432008
//@ run-call: Ci8_0::t_local_encode_selector => 37586276945623995216024172036549658488059665540187076971129608637875230052014
//@ run-call: Ci8_0::t_retdirect_encode_selector => 37586276945623995216024172036549658488059665540187076971129608637875230052014
//@ run-call: Ci8_0::t_param_encode_selector => 37586276945623995216024172036549658488059665540187076971129608637875230052014
//@ run-call: Ci8_0::t_mem_encode_selector => 37586276945623995216024172036549658488059665540187076971129608637875230052014
//@ run-call: Ci8_0::t_local_encode_signature => 16251987676518111741736386623807977930197340032924352303410262046802744800769
//@ run-call: Ci8_0::t_retdirect_encode_signature => 16251987676518111741736386623807977930197340032924352303410262046802744800769
//@ run-call: Ci8_0::t_param_encode_signature => 16251987676518111741736386623807977930197340032924352303410262046802744800769
//@ run-call: Ci8_0::t_mem_encode_signature => 16251987676518111741736386623807977930197340032924352303410262046802744800769
//@ run-call: Ci8_0::t_local_encode_call => 63164772549884817474649791266019118906958472107833536650064371408703643498696
//@ run-call: Ci8_0::t_retdirect_encode_call => 63164772549884817474649791266019118906958472107833536650064371408703643498696
//@ run-call: Ci8_0::t_param_encode_call => 63164772549884817474649791266019118906958472107833536650064371408703643498696
//@ run-call: Ci8_0::t_mem_encode_call => 63164772549884817474649791266019118906958472107833536650064371408703643498696
//@ run-call: Ci24_0::t_local_encode_selector => 37586276945623995216024172036549658488059665540187076971129608637875230052014
//@ run-call: Ci24_0::t_retdirect_encode_selector => 37586276945623995216024172036549658488059665540187076971129608637875230052014
//@ run-call: Ci24_0::t_param_encode_selector => 37586276945623995216024172036549658488059665540187076971129608637875230052014
//@ run-call: Ci24_0::t_mem_encode_selector => 37586276945623995216024172036549658488059665540187076971129608637875230052014
//@ run-call: Ci24_0::t_local_encode_signature => 5995037954818866630810161555064903198045056582575244644445189867362709504867
//@ run-call: Ci24_0::t_retdirect_encode_signature => 5995037954818866630810161555064903198045056582575244644445189867362709504867
//@ run-call: Ci24_0::t_param_encode_signature => 5995037954818866630810161555064903198045056582575244644445189867362709504867
//@ run-call: Ci24_0::t_mem_encode_signature => 5995037954818866630810161555064903198045056582575244644445189867362709504867
//@ run-call: Ci24_0::t_local_encode_call => 108159033926175892190334844489410625027724111191058122749770109428413421885286
//@ run-call: Ci24_0::t_retdirect_encode_call => 108159033926175892190334844489410625027724111191058122749770109428413421885286
//@ run-call: Ci24_0::t_param_encode_call => 108159033926175892190334844489410625027724111191058122749770109428413421885286
//@ run-call: Ci24_0::t_mem_encode_call => 108159033926175892190334844489410625027724111191058122749770109428413421885286
//@ run-call: Caddr_0::t_local_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Caddr_0::t_retdirect_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Caddr_0::t_param_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Caddr_0::t_mem_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Caddr_0::t_local_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Caddr_0::t_retdirect_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Caddr_0::t_param_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Caddr_0::t_mem_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Caddr_0::t_local_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Caddr_0::t_retdirect_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Caddr_0::t_param_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Caddr_0::t_mem_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Caddrp_0::t_local_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Caddrp_0::t_retdirect_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Caddrp_0::t_param_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Caddrp_0::t_mem_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Caddrp_0::t_local_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Caddrp_0::t_retdirect_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Caddrp_0::t_param_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Caddrp_0::t_mem_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Caddrp_0::t_local_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Caddrp_0::t_retdirect_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Caddrp_0::t_param_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Caddrp_0::t_mem_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Cctr_0::t_local_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Cctr_0::t_retdirect_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Cctr_0::t_param_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Cctr_0::t_mem_encode_selector => 114023740480644931902476397314330374871035631518752294340137274177861238719479
//@ run-call: Cctr_0::t_local_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Cctr_0::t_retdirect_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Cctr_0::t_param_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Cctr_0::t_mem_encode_signature => 40738862819796907872721924206667675221623200985294213949465934303000777383498
//@ run-call: Cctr_0::t_local_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Cctr_0::t_retdirect_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Cctr_0::t_param_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Cctr_0::t_mem_encode_call => 6334443833447129599731285318330928413164461044054734133699745702745046304289
//@ run-call: Cbool_0::t_local_encode_selector => 8189486464072686116219159991641131467043951551971549668685891144623196419153
//@ run-call: Cbool_0::t_retdirect_encode_selector => 8189486464072686116219159991641131467043951551971549668685891144623196419153
//@ run-call: Cbool_0::t_param_encode_selector => 8189486464072686116219159991641131467043951551971549668685891144623196419153
//@ run-call: Cbool_0::t_mem_encode_selector => 8189486464072686116219159991641131467043951551971549668685891144623196419153
//@ run-call: Cbool_0::t_local_encode_signature => 88704208415822373873988945890373875195694579275563657728752380001546006910695
//@ run-call: Cbool_0::t_retdirect_encode_signature => 88704208415822373873988945890373875195694579275563657728752380001546006910695
//@ run-call: Cbool_0::t_param_encode_signature => 88704208415822373873988945890373875195694579275563657728752380001546006910695
//@ run-call: Cbool_0::t_mem_encode_signature => 88704208415822373873988945890373875195694579275563657728752380001546006910695
//@ run-call: Cbool_0::t_local_encode_call => 88114199493485027560379247874403499690499040921946239342075200804755308911941
//@ run-call: Cbool_0::t_retdirect_encode_call => 88114199493485027560379247874403499690499040921946239342075200804755308911941
//@ run-call: Cbool_0::t_param_encode_call => 88114199493485027560379247874403499690499040921946239342075200804755308911941
//@ run-call: Cbool_0::t_mem_encode_call => 88114199493485027560379247874403499690499040921946239342075200804755308911941
//@ run-call: Cb1_0::t_local_encode_selector => 37876372681676112452529768248117194768173895775916205080173667549092624401292
//@ run-call: Cb1_0::t_retdirect_encode_selector => 37876372681676112452529768248117194768173895775916205080173667549092624401292
//@ run-call: Cb1_0::t_param_encode_selector => 37876372681676112452529768248117194768173895775916205080173667549092624401292
//@ run-call: Cb1_0::t_mem_encode_selector => 37876372681676112452529768248117194768173895775916205080173667549092624401292
//@ run-call: Cb1_0::t_local_encode_signature => 22405581775250056100257562002896928454387615927749539978992887583423162311587
//@ run-call: Cb1_0::t_retdirect_encode_signature => 22405581775250056100257562002896928454387615927749539978992887583423162311587
//@ run-call: Cb1_0::t_param_encode_signature => 22405581775250056100257562002896928454387615927749539978992887583423162311587
//@ run-call: Cb1_0::t_mem_encode_signature => 22405581775250056100257562002896928454387615927749539978992887583423162311587
//@ run-call: Cb1_0::t_local_encode_call => 110170711462164813989966157202070012047889335748981414781117515390180124865748
//@ run-call: Cb1_0::t_retdirect_encode_call => 110170711462164813989966157202070012047889335748981414781117515390180124865748
//@ run-call: Cb1_0::t_param_encode_call => 110170711462164813989966157202070012047889335748981414781117515390180124865748
//@ run-call: Cb1_0::t_mem_encode_call => 110170711462164813989966157202070012047889335748981414781117515390180124865748
//@ run-call: Cb4_0::t_local_encode_selector => 29343788489913391870595477345229017687465609421185891528892209945606898766605
//@ run-call: Cb4_0::t_retdirect_encode_selector => 29343788489913391870595477345229017687465609421185891528892209945606898766605
//@ run-call: Cb4_0::t_param_encode_selector => 29343788489913391870595477345229017687465609421185891528892209945606898766605
//@ run-call: Cb4_0::t_mem_encode_selector => 29343788489913391870595477345229017687465609421185891528892209945606898766605
//@ run-call: Cb4_0::t_local_encode_signature => 53432823365466753302249332946136455478879653326100583918201738501463277284003
//@ run-call: Cb4_0::t_retdirect_encode_signature => 53432823365466753302249332946136455478879653326100583918201738501463277284003
//@ run-call: Cb4_0::t_param_encode_signature => 53432823365466753302249332946136455478879653326100583918201738501463277284003
//@ run-call: Cb4_0::t_mem_encode_signature => 53432823365466753302249332946136455478879653326100583918201738501463277284003
//@ run-call: Cb4_0::t_local_encode_call => 23447380703215291950344074193778715024621793582755157430070166923622972845376
//@ run-call: Cb4_0::t_retdirect_encode_call => 23447380703215291950344074193778715024621793582755157430070166923622972845376
//@ run-call: Cb4_0::t_param_encode_call => 23447380703215291950344074193778715024621793582755157430070166923622972845376
//@ run-call: Cb4_0::t_mem_encode_call => 23447380703215291950344074193778715024621793582755157430070166923622972845376
//@ run-call: Cb20_0::t_local_encode_selector => 90455653940308664117452794923778953580680841655354063161629465380160442137753
//@ run-call: Cb20_0::t_retdirect_encode_selector => 90455653940308664117452794923778953580680841655354063161629465380160442137753
//@ run-call: Cb20_0::t_param_encode_selector => 90455653940308664117452794923778953580680841655354063161629465380160442137753
//@ run-call: Cb20_0::t_mem_encode_selector => 90455653940308664117452794923778953580680841655354063161629465380160442137753
//@ run-call: Cb20_0::t_local_encode_signature => 59716159290399845845301118597795420597056146227374585882764325616493469018417
//@ run-call: Cb20_0::t_retdirect_encode_signature => 59716159290399845845301118597795420597056146227374585882764325616493469018417
//@ run-call: Cb20_0::t_param_encode_signature => 59716159290399845845301118597795420597056146227374585882764325616493469018417
//@ run-call: Cb20_0::t_mem_encode_signature => 59716159290399845845301118597795420597056146227374585882764325616493469018417
//@ run-call: Cb20_0::t_local_encode_call => 5231741069037052881251213407555035980151281904563299750676575708923395498863
//@ run-call: Cb20_0::t_retdirect_encode_call => 5231741069037052881251213407555035980151281904563299750676575708923395498863
//@ run-call: Cb20_0::t_param_encode_call => 5231741069037052881251213407555035980151281904563299750676575708923395498863
//@ run-call: Cb20_0::t_mem_encode_call => 5231741069037052881251213407555035980151281904563299750676575708923395498863
//@ run-call-fail: Cen_0::t_local_encode_selector => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode_selector => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode_selector => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode_selector => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_encode_signature => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode_signature => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode_signature => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode_signature => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_encode_call => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_encode_call => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_encode_call => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_encode_call => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cud_0::t_retdirect_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cud_0::t_param_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cud_0::t_mem_encode_selector => 12150456603831802461460141978385352242678830973451299182662405126869665675876
//@ run-call: Cud_0::t_local_encode_signature => 87186003289801191604053496905270320844958310796545158597700757130929743847240
//@ run-call: Cud_0::t_retdirect_encode_signature => 87186003289801191604053496905270320844958310796545158597700757130929743847240
//@ run-call: Cud_0::t_param_encode_signature => 87186003289801191604053496905270320844958310796545158597700757130929743847240
//@ run-call: Cud_0::t_mem_encode_signature => 87186003289801191604053496905270320844958310796545158597700757130929743847240
//@ run-call: Cud_0::t_local_encode_call => 8531465725870555205293184461453974544980304869217186201431725384886034049810
//@ run-call: Cud_0::t_retdirect_encode_call => 8531465725870555205293184461453974544980304869217186201431725384886034049810
//@ run-call: Cud_0::t_param_encode_call => 8531465725870555205293184461453974544980304869217186201431725384886034049810
//@ run-call: Cud_0::t_mem_encode_call => 8531465725870555205293184461453974544980304869217186201431725384886034049810

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: ABI encodings and custom errors.

interface IC {
    function f() external;
}

enum E {
    A,
    B,
    C
}

type U is uint8;

interface IF_u8 {
    function g(uint8) external;
}

interface IF_u64 {
    function g(uint64) external;
}

interface IF_i8 {
    function g(int8) external;
}

interface IF_i24 {
    function g(int24) external;
}

interface IF_addr {
    function g(address) external;
}

interface IF_addrp {
    function g(address payable) external;
}

interface IF_ctr {
    function g(IC) external;
}

interface IF_bool {
    function g(bool) external;
}

interface IF_b1 {
    function g(bytes1) external;
}

interface IF_b4 {
    function g(bytes4) external;
}

interface IF_b20 {
    function g(bytes20) external;
}

interface IF_en {
    function g(E) external;
}

interface IF_ud {
    function g(U) external;
}

contract Cu8_0 {
    function _s_retdirect_encode_selector() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_selector(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_signature(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(uint8)", pv))); }
    function _s_retdirect_encode_call() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_call(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_u8.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodeWithSignature("f(uint8)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(uint8)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(uint8)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodeCall(IF_u8.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_u8.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_u8.g, (ry)))); }
}
contract Cu64_0 {
    function _s_retdirect_encode_selector() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode_selector(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode_signature(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(uint64)", pv))); }
    function _s_retdirect_encode_call() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_encode_call(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_u64.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encodeWithSignature("f(uint64)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(uint64)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(uint64)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encodeCall(IF_u64.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_u64.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_u64.g, (ry)))); }
}
contract Ci8_0 {
    function _s_retdirect_encode_selector() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode_selector(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode_signature(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(int8)", pv))); }
    function _s_retdirect_encode_call() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_encode_call(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_i8.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encodeWithSignature("f(int8)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(int8)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(int8)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encodeCall(IF_i8.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_i8.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_i8.g, (ry)))); }
}
contract Ci24_0 {
    function _s_retdirect_encode_selector() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode_selector(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode_signature(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(int24)", pv))); }
    function _s_retdirect_encode_call() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_encode_call(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_i24.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encodeWithSignature("f(int24)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(int24)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(int24)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encodeCall(IF_i24.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_i24.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_i24.g, (ry)))); }
}
contract Caddr_0 {
    function _s_retdirect_encode_selector() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_selector(address pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_signature(address pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(address)", pv))); }
    function _s_retdirect_encode_call() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_call(address pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_addr.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodeWithSignature("f(address)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(address)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(address)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodeCall(IF_addr.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_addr.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_addr.g, (ry)))); }
}
contract Caddrp_0 {
    function _s_retdirect_encode_selector() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_selector(address payable pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_signature(address payable pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(address)", pv))); }
    function _s_retdirect_encode_call() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_call(address payable pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_addrp.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodeWithSignature("f(address)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(address)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(address)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodeCall(IF_addrp.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_addrp.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_addrp.g, (ry)))); }
}
contract Cctr_0 {
    function _s_retdirect_encode_selector() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_selector(IC pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_signature(IC pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(address)", pv))); }
    function _s_retdirect_encode_call() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_encode_call(IC pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_ctr.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodeWithSignature("f(address)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(address)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(address)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(keccak256(abi.encodeCall(IF_ctr.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_ctr.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_ctr.g, (ry)))); }
}
contract Cbool_0 {
    function _s_retdirect_encode_selector() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode_selector(bool pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode_signature(bool pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(bool)", pv))); }
    function _s_retdirect_encode_call() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_encode_call(bool pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_bool.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(keccak256(abi.encodeWithSignature("f(bool)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(bool)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(bool)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(keccak256(abi.encodeCall(IF_bool.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_bool.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_bool.g, (ry)))); }
}
contract Cb1_0 {
    function _s_retdirect_encode_selector() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode_selector(bytes1 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode_signature(bytes1 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(bytes1)", pv))); }
    function _s_retdirect_encode_call() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_encode_call(bytes1 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_b1.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(keccak256(abi.encodeWithSignature("f(bytes1)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(bytes1)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(bytes1)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(keccak256(abi.encodeCall(IF_b1.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_b1.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_b1.g, (ry)))); }
}
contract Cb4_0 {
    function _s_retdirect_encode_selector() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode_selector(bytes4 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode_signature(bytes4 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(bytes4)", pv))); }
    function _s_retdirect_encode_call() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_encode_call(bytes4 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_b4.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(keccak256(abi.encodeWithSignature("f(bytes4)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(bytes4)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(bytes4)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(keccak256(abi.encodeCall(IF_b4.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_b4.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_b4.g, (ry)))); }
}
contract Cb20_0 {
    function _s_retdirect_encode_selector() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode_selector(bytes20 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode_signature(bytes20 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(bytes20)", pv))); }
    function _s_retdirect_encode_call() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_encode_call(bytes20 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_b20.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(keccak256(abi.encodeWithSignature("f(bytes20)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(bytes20)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(bytes20)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(keccak256(abi.encodeCall(IF_b20.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_b20.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_b20.g, (ry)))); }
}
contract Cen_0 {
    function _s_retdirect_encode_selector() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode_selector(E pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode_signature(E pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(uint8)", pv))); }
    function _s_retdirect_encode_call() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_encode_call(E pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_en.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(keccak256(abi.encodeWithSignature("f(uint8)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(uint8)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(uint8)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(keccak256(abi.encodeCall(IF_en.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_en.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_en.g, (ry)))); }
}
contract Cud_0 {
    function _s_retdirect_encode_selector() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_selector(U pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, pv))); }
    function _s_retdirect_encode_signature() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_signature(U pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(uint8)", pv))); }
    function _s_retdirect_encode_call() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_encode_call(U pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_ud.g, (pv)))); }
    function t_local_encode_selector() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodeWithSelector(0x12345678, rx))); }
    function t_retdirect_encode_selector() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSelector(0x12345678, _s_retdirect_encode_selector()))); }
    function t_param_encode_selector() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode_selector(rx); }
    function t_mem_encode_selector() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(keccak256(abi.encodeWithSelector(0x12345678, ry))); }
    function t_local_encode_signature() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodeWithSignature("f(uint8)", rx))); }
    function t_retdirect_encode_signature() external view  returns (uint256) { return uint256(keccak256(abi.encodeWithSignature("f(uint8)", _s_retdirect_encode_signature()))); }
    function t_param_encode_signature() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode_signature(rx); }
    function t_mem_encode_signature() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(keccak256(abi.encodeWithSignature("f(uint8)", ry))); }
    function t_local_encode_call() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodeCall(IF_ud.g, (rx)))); }
    function t_retdirect_encode_call() external view  returns (uint256) { return uint256(keccak256(abi.encodeCall(IF_ud.g, (_s_retdirect_encode_call())))); }
    function t_param_encode_call() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_encode_call(rx); }
    function t_mem_encode_call() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(keccak256(abi.encodeCall(IF_ud.g, (ry)))); }
}
