function roundedChange(value: number) {
  return Math.round(value * 100) / 100
}

export function changeClass(value: number | null) {
  if (value === null) return 'neutral'
  const rounded = roundedChange(value)
  return rounded < 0 ? 'bad' : rounded > 0 ? 'good' : 'neutral'
}

export function formatChange(value: number | null, fallback = '') {
  if (value === null) return fallback
  const rounded = roundedChange(value)
  return `${rounded > 0 ? '+' : ''}${rounded.toFixed(2)}%`
}
