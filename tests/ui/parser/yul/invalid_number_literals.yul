// ported-from: test/libyul/yulSyntaxTests/number_literals_2.yul
// ported-from: test/libyul/yulSyntaxTests/number_literals_3.yul
// ported-from: test/libyul/yulSyntaxTests/number_literals_4.yul
{
	let x := .1 //~ ERROR: invalid number literal
	let y := 1e5 //~ ERROR: invalid number literal
	let z := 67.235 //~ ERROR: invalid number literal
}
