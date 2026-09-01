import { useEffect, useMemo, useState } from 'react'
import { loadIndex } from './data'
import type { RunIndex, RunSummary } from './types'
import { Compare } from './Compare'

const short = (commit: string) => commit.slice(0, 8)

function HistoryGraph({ runs }: { runs: RunSummary[] }) {
  const points = runs
    .filter((run) => run.metrics.runtimeGas !== null)
    .slice(0, 40)
    .reverse()
  if (points.length < 2) return <div className="empty-graph">History appears after two benchmarked commits.</div>
  const values = points.map((run) => run.metrics.runtimeGas!)
  const min = Math.min(...values)
  const max = Math.max(...values)
  const range = max - min || 1
  const path = points.map((run, index) => {
    const x = (index / (points.length - 1)) * 100
    const y = 88 - ((run.metrics.runtimeGas! - min) / range) * 72
    return `${index ? 'L' : 'M'} ${x} ${y}`
  }).join(' ')
  return (
    <svg className="history" viewBox="0 0 100 100" preserveAspectRatio="none" role="img" aria-label="Runtime gas history">
      <path className="grid" d="M0 16H100 M0 52H100 M0 88H100" />
      <path className="series" d={path} />
    </svg>
  )
}

export function App() {
  const route = new URLSearchParams(window.location.search)
  const baseCommit = route.get('base')
  const headCommit = route.get('head')
  if (baseCommit && headCommit && baseCommit !== headCommit) return <><SiteHeader /><Compare base={baseCommit} head={headCommit} /><SiteFooter /></>
  return <Home />
}

function SiteHeader() {
  return <header><a className="wordmark" href={import.meta.env.BASE_URL}>solar<span>/perf</span></a><nav><a href="https://github.com/paradigmxyz/solar">repository ↗</a></nav></header>
}

function SiteFooter() { return <footer>Measured by the in-repository runtime corpus.</footer> }

function Home() {
  const [index, setIndex] = useState<RunIndex | null>(null)
  const [error, setError] = useState('')
  const [base, setBase] = useState('')
  const [head, setHead] = useState('')
  const runs = useMemo(() => index?.runs ?? [], [index])
  const options = useMemo(() => runs.map((run) => run.commit), [runs])

  useEffect(() => { loadIndex().then(setIndex).catch((value: Error) => setError(value.message)) }, [])
  useEffect(() => {
    if (options.length && !head) setHead(options[0])
    if (options.length > 1 && !base) setBase(options[1])
  }, [base, head, options])

  const compare = () => {
    if (!base || !head || base === head) return
    const url = new URL(window.location.href)
    url.search = new URLSearchParams({ base, head }).toString()
    window.location.href = url.toString()
  }

  return (
    <>
      <SiteHeader />
      <main>
        <section className="intro">
          <p className="eyebrow">Compiler performance</p>
          <h1>Track output quality.<br />Inspect every change.</h1>
          <p className="lede">Gas, bytecode, compile time, and the compiler artifacts behind each result.</p>
        </section>
        <section className="compare-box" aria-label="Compare commits">
          <datalist id="commits">{runs.map((run) => <option key={run.commit} value={run.commit}>{short(run.commit)} · {run.branch ?? 'detached'}</option>)}</datalist>
          <label>base<input list="commits" placeholder="Commit SHA" value={base} onChange={(event) => setBase(event.target.value)} /></label>
          <span className="arrow">→</span>
          <label>head<input list="commits" placeholder="Commit SHA" value={head} onChange={(event) => setHead(event.target.value)} /></label>
          <button onClick={compare} disabled={!base || !head || base === head}>Compare</button>
        </section>
        <section className="panel graph-panel">
          <div className="panel-heading"><div><p className="eyebrow">Main branch</p><h2>Runtime gas</h2></div><span>{runs.length} runs</span></div>
          {error ? <p className="error">{error}</p> : <HistoryGraph runs={runs} />}
        </section>
        <section className="recent">
          <div className="panel-heading"><h2>Recent runs</h2><span>lower is better</span></div>
          {runs.length === 0 ? <p className="empty">No published benchmark runs yet.</p> : runs.slice(0, 8).map((run) => <a className="run" key={run.commit} href={`?base=${runs[1]?.commit ?? run.commit}&head=${run.commit}`}><code>{short(run.commit)}</code><span>{run.branch ?? `PR #${run.pr}`}</span><strong>{run.metrics.runtimeGas?.toLocaleString() ?? 'n/a'} gas</strong></a>)}
        </section>
      </main>
      <SiteFooter />
    </>
  )
}
