import { useEffect, useState } from 'react'
import { loadIndex, loadRun } from './data'
import type { BenchmarkResult } from './types'

interface Props {
  benchmark: string
  metric: string
}

function metricValue(result: BenchmarkResult | undefined, metric: string) {
  const value = result?.compilers.solar?.[metric as keyof BenchmarkResult['compilers']['solar']]
  return typeof value === 'number' ? value : null
}

export function BenchmarkHistory({ benchmark, metric }: Props) {
  const [values, setValues] = useState<number[] | null>(null)

  useEffect(() => {
    let cancelled = false
    loadIndex()
      .then((index) => Promise.all(index.runs.slice(0, 24).reverse().map((run) => loadRun(run.commit))))
      .then((runs) => {
        if (!cancelled) setValues(runs.map((run) => metricValue(run.results.find((result) => result.test_id === benchmark), metric)).filter((value): value is number => value !== null))
      })
      .catch(() => { if (!cancelled) setValues([]) })
    return () => { cancelled = true }
  }, [benchmark, metric])

  if (values === null) return <p className="detail-muted">Loading history…</p>
  if (values.length < 2) return <p className="detail-muted">No benchmark history is available yet.</p>
  const min = Math.min(...values)
  const max = Math.max(...values)
  const range = max - min
  const path = values.map((value, index) => `${index ? 'L' : 'M'} ${(index / (values.length - 1)) * 100} ${range === 0 ? 50 : 90 - ((value - min) / range) * 80}`).join(' ')
  return <svg className="detail-history" viewBox="0 0 100 100" preserveAspectRatio="none" role="img" aria-label="Benchmark history"><path className="grid" d="M0 10H100 M0 50H100 M0 90H100" /><path className="series" d={path} /></svg>
}
