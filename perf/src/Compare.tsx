import { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import { changeClass, formatChange } from './change'
import { loadRun } from './data'
import type { ArtifactFile, BenchmarkResult, RunDocument, Theme } from './types'

const ArtifactDiff = lazy(() => import('./ArtifactDiff'))
const metrics = {
  runtimeGas: ['Runtime gas', 'total_gas'],
  deployGas: ['Deploy gas', 'deploy_gas'],
  runtimeSize: ['Runtime bytes', 'runtime_size'],
  creationSize: ['Creation bytes', 'bytecode_size'],
  compileTime: ['Compile time', 'compile_time_seconds'],
} as const

function value(result: BenchmarkResult | undefined, key: string) {
  const compiler = result?.compilers.solar
  const metric = compiler?.[key as keyof typeof compiler]
  return typeof metric === 'number' ? metric : null
}

function change(before: number | null, after: number | null) {
  if (before === null || after === null || before === 0) return null
  return ((after - before) / before) * 100
}

export function Compare({ base, head, theme }: { base: string; head: string; theme: Theme }) {
  const [runs, setRuns] = useState<[RunDocument, RunDocument] | null>(null)
  const [loadFailed, setLoadFailed] = useState(false)
  const [query, setQuery] = useState('')
  const params = new URLSearchParams(window.location.search)
  const [selected, setSelected] = useState(params.get('benchmark') ?? '')
  const [metric, setMetric] = useState<keyof typeof metrics>('runtimeGas')
  const [against, setAgainst] = useState<'base' | 'solc'>('base')
  const [artifact, setArtifact] = useState('')
  useEffect(() => {
    let cancelled = false
    setRuns(null)
    setLoadFailed(false)
    Promise.all([loadRun(base), loadRun(head)])
      .then((loaded) => { if (!cancelled) setRuns(loaded) })
      .catch(() => { if (!cancelled) setLoadFailed(true) })
    return () => { cancelled = true }
  }, [base, head])
  const rows = useMemo(() => {
    if (!runs) return []
    const before = new Map(runs[0].results.map((result) => [result.test_id, result]))
    const key = metrics[metric][1]
    return runs[1].results
      .map((after) => ({ before: before.get(after.test_id), after }))
      .filter(({ before, after }) => after.test_id.toLowerCase().includes(query.toLowerCase()) && (value(before, key) !== null || value(after, key) !== null))
  }, [metric, query, runs])
  if (loadFailed) return <main className="compare-page"><a className="back" href={import.meta.env.BASE_URL}>← history</a><section className="load-error"><p className="eyebrow">Comparison unavailable</p><h1>Benchmark data not found</h1><p>One or both commits have not been published yet.</p></section></main>
  if (!runs) return <main><p className="empty">Loading comparison…</p></main>
  const [beforeRun, afterRun] = runs
  const files: ArtifactFile[] = afterRun.artifacts[selected] ?? beforeRun.artifacts[selected] ?? []
  const comparisonCommit = against === 'base' ? base : head
  const comparisonCompiler = against === 'base' ? 'solar' : 'solc'
  const selectedFile = files.find((file) => file.path === artifact)
  return <main className="compare-page">
    <a className="back" href={import.meta.env.BASE_URL}>← history</a>
    <div className="compare-title"><div><p className="eyebrow">Commit comparison</p><h1>{base.slice(0, 8)} <span>→</span> {head.slice(0, 8)}</h1></div><a href={`https://github.com/paradigmxyz/solar/compare/${base}...${head}`}>source diff ↗</a></div>
    <div className="filters"><input aria-label="Filter benchmarks" placeholder="Filter benchmarks…" value={query} onChange={(event) => setQuery(event.target.value)} /><select value={metric} onChange={(event) => setMetric(event.target.value as keyof typeof metrics)}>{Object.entries(metrics).map(([key, [label]]) => <option key={key} value={key}>{label}</option>)}</select></div>
    <section id="benchmarks" className="results"><div className="result header-row"><span>benchmark</span><span>base</span><span>head</span><span>change</span></div>{rows.map(({ before, after }) => { const a = value(before, metrics[metric][1]); const b = value(after, metrics[metric][1]); const delta = change(a, b); return <button className={`result ${selected === after.test_id ? 'selected' : ''}`} key={after.test_id} onClick={() => { setSelected(after.test_id); setArtifact(''); const url = new URL(window.location.href); url.searchParams.set('benchmark', after.test_id); history.replaceState(null, '', url) }}><code>{after.test_id}</code><span>{a?.toLocaleString() ?? 'n/a'}</span><span>{b?.toLocaleString() ?? 'n/a'}</span><strong className={changeClass(delta)}>{formatChange(delta, 'n/a')}</strong></button>})}</section>
    {selected && <section id="artifacts" className="artifacts"><div className="artifact-head"><div><p className="eyebrow">Artifacts</p><h2>{selected}</h2></div>{files.length > 0 && <div className="toggle"><button className={against === 'base' ? 'active' : ''} onClick={() => { setAgainst('base'); setArtifact('') }}>vs base</button><button className={against === 'solc' ? 'active' : ''} onClick={() => { setAgainst('solc'); setArtifact('') }}>vs solc</button></div>}</div>{files.length > 0 ? <div className="artifact-body"><aside>{files.map((file) => <button key={file.path} className={artifact === file.path ? 'active' : ''} onClick={() => setArtifact(file.path)}><span>{file.label}</span><small>{(file.bytes / 1024).toFixed(1)} KiB</small></button>)}</aside><div className="diff-frame">{selectedFile ? <Suspense fallback={<p className="empty">Loading renderer…</p>}><ArtifactDiff before={{ commit: comparisonCommit, benchmark: selected, compiler: comparisonCompiler }} after={{ commit: head, benchmark: selected, compiler: 'solar' }} path={selectedFile.path} storagePath={selectedFile.storagePath} language={selectedFile.language} theme={theme} /></Suspense> : <p className="empty">Choose an artifact to inspect its diff.</p>}</div></div> : <p className="artifact-empty">No artifacts were published for this benchmark run.</p>}</section>}
  </main>
}
