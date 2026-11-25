import SolarTV.EVMIR.Precompiles.Bytes

/-! Precompile gas schedules and MSM discount tables. ModExp follows EIP-198,
Berlin EIP-2565 and Osaka EIP-7883; the Osaka size restriction is checked before
evaluation by the dispatcher. BN254 repricing starts at Istanbul. BLS discounts
are EIP-2537's exact integer tables, saturated at the final entry.
https://eips.ethereum.org/EIPS/eip-198
https://eips.ethereum.org/EIPS/eip-2565
https://eips.ethereum.org/EIPS/eip-7883
https://eips.ethereum.org/EIPS/eip-2537#gas-schedule -/

namespace SolarTV.Precompiles
open EVM.State

def discount1 : List Nat := [
  1000,949,848,797,764,750,738,728,719,712,705,698,692,687,682,677,673,669,665,
  661,658,654,651,648,645,642,640,637,635,632,630,627,625,623,621,619,617,615,
  613,611,609,608,606,604,603,601,599,598,596,595,593,592,591,589,588,586,585,
  584,582,581,580,579,577,576,575,574,573,572,570,569,568,567,566,565,564,563,
  562,561,560,559,558,557,556,555,554,553,552,551,550,549,548,547,547,546,545,
  544,543,542,541,540,540,539,538,537,536,536,535,534,533,532,532,531,530,529,
  528,528,527,526,525,525,524,523,522,522,521,520,520,519]
def discount2 : List Nat := [
  1000,1000,923,884,855,832,812,796,782,770,759,749,740,732,724,717,711,704,
  699,693,688,683,679,674,670,666,663,659,655,652,649,646,643,640,637,634,632,
  629,627,624,622,620,618,615,613,611,609,607,606,604,602,600,598,597,595,593,
  592,590,589,587,586,584,583,582,580,579,578,576,575,574,573,571,570,569,568,
  567,566,565,563,562,561,560,559,558,557,556,555,554,553,552,552,551,550,549,
  548,547,546,545,545,544,543,542,541,541,540,539,538,537,537,536,535,535,534,
  533,532,532,531,530,530,529,528,528,527,526,526,525,524,524]

def modexpPrice (fork : Fork) (input : List Byte) : Nat :=
  let base := readBE input 0 32
  let exponent := readBE input 32 32
  let modulus := readBE input 64 32
  let head := readBE input (96+base) (min exponent 32)
  let length := max base modulus
  let iterations := max 1 ((if fork.atLeast .osaka then 16 else 8)*(exponent-32) + head.log2)
  if fork.atLeast .osaka then
    max 500 ((if length ≤ 32 then 16 else 2*((length+7)/8)^2)*iterations)
  else if fork.atLeast .berlin then max 200 (((length+7)/8)^2 * iterations / 3)
  else
    let complexity := if length ≤ 64 then length^2
      else if length ≤ 1024 then length^2/4 + 96*length - 3072
      else length^2/16 + 480*length - 199680
    complexity * iterations / 20

def price (fork : Fork) (address : Nat) (input : List Byte) : Nat :=
  let words := (input.length+31)/32
  match address with
  | 1 => 3000
  | 2 => 60 + 12*words
  | 3 => 600 + 120*words
  | 4 => 15 + 3*words
  | 5 => modexpPrice fork input
  | 6 => if fork.atLeast .istanbul then 150 else 500
  | 7 => if fork.atLeast .istanbul then 6000 else 40000
  | 8 => if fork.atLeast .istanbul then 45000 + 34000*(input.length/192)
      else 100000 + 80000*(input.length/192)
  | 9 => readBE input 0 4
  | 10 => 50000
  | 11 => 375
  | 12 => let k := input.length/160
          12000*k*(discount1[min (k-1) 127]?.getD 519)/1000
  | 13 => 600
  | 14 => let k := input.length/288
          22500*k*(discount2[min (k-1) 127]?.getD 524)/1000
  | 15 => 37700 + 32600*(input.length/384)
  | 16 => 5500
  | 17 => 23800
  | 256 => 6900
  | _ => 0

end SolarTV.Precompiles
