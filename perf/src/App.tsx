import { useEffect, useMemo, useState } from 'react'
import { Moon, Sun } from 'lucide-react'
import { changeClass, formatChange } from './change'
import { loadIndex } from './data'
import type { MetricSummary, RunIndex, RunSummary, Theme } from './types'
import { Compare } from './Compare'

const short = (commit: string) => commit.slice(0, 8)

const charts: { metric: keyof MetricSummary; title: string; unit: string }[] = [
  { metric: 'runtimeGas', title: 'Runtime gas', unit: 'gas' },
  { metric: 'deployGas', title: 'Deployment gas', unit: 'gas' },
  { metric: 'runtimeSize', title: 'Runtime bytecode', unit: 'bytes' },
  { metric: 'creationSize', title: 'Creation bytecode', unit: 'bytes' },
  { metric: 'compileTime', title: 'Compile time', unit: 'seconds' },
]

function formatValue(value: number, unit: string) {
  if (unit === 'seconds') return `${value.toFixed(2)} s`
  return `${Math.round(value).toLocaleString()} ${unit}`
}

function runRef(run: RunSummary) {
  return run.branch ?? (run.pr ? `PR #${run.pr}` : 'detached')
}

function runLabel(run: RunSummary) {
  return `${short(run.commit)} · ${runRef(run)} · ${new Date(run.timestamp).toLocaleDateString()}`
}

function HistoryGraph({ runs, metric, title, unit }: { runs: RunSummary[]; metric: keyof MetricSummary; title: string; unit: string }) {
  const points = runs
    .filter((run) => run.branch === 'main' && run.metrics[metric] !== null)
    .slice(0, 60)
    .reverse()
  const values = points.map((run) => run.metrics[metric]!)
  const min = Math.min(...values)
  const max = Math.max(...values)
  const range = max - min
  const path = points.map((run, index) => {
    const x = (index / Math.max(points.length - 1, 1)) * 100
    const y = range === 0 ? 50 : 88 - ((run.metrics[metric]! - min) / range) * 76
    return `${index ? 'L' : 'M'} ${x} ${y}`
  }).join(' ')
  const first = values[0]
  const latest = values.at(-1)
  const change = first && latest !== undefined ? ((latest - first) / first) * 100 : null
  return (
    <section className="graph-card">
      <div className="graph-heading">
        <h2>{title}</h2>
        {latest !== undefined && <div><strong>{formatValue(latest, unit)}</strong><span className={changeClass(change)}>{formatChange(change)}</span></div>}
      </div>
      {points.length < 2 ? <div className="empty-graph">Waiting for two main-branch runs.</div> : <>
        <div className="chart-body">
          <div className="chart-scale"><span>{formatValue(max, unit)}</span><span>{formatValue(min, unit)}</span></div>
          <svg className="history" viewBox="0 0 100 100" preserveAspectRatio="none" role="img" aria-label={`${title} over time`}>
            <path className="grid" d="M0 12H100 M0 50H100 M0 88H100" />
            <path className="series" d={path} />
          </svg>
        </div>
        <div className="chart-dates"><span>{new Date(points[0].timestamp).toLocaleDateString()}</span><span>{new Date(points.at(-1)!.timestamp).toLocaleDateString()}</span></div>
      </>}
    </section>
  )
}

export function App() {
  const [theme, setTheme] = useState<Theme>(() => {
    const initial = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
    document.documentElement.dataset.theme = initial
    return initial
  })
  useEffect(() => {
    document.documentElement.dataset.theme = theme
  }, [theme])
  const route = new URLSearchParams(window.location.search)
  const baseCommit = route.get('base')
  const headCommit = route.get('head')
  const content = baseCommit && headCommit && baseCommit !== headCommit ? <Compare base={baseCommit} head={headCommit} theme={theme} /> : <Home />
  return <><SiteHeader theme={theme} onToggleTheme={() => setTheme((value) => value === 'light' ? 'dark' : 'light')} />{content}<SiteFooter /></>
}

function SiteHeader({ theme, onToggleTheme }: { theme: Theme; onToggleTheme: () => void }) {
  const nextTheme = theme === 'light' ? 'dark' : 'light'
  return <header><a className="wordmark" href={import.meta.env.BASE_URL}>solar<span>/perf</span></a><nav><button className="theme-toggle" onClick={onToggleTheme} aria-label={`Switch to ${nextTheme} theme`} title={`Switch to ${nextTheme} theme`}>{theme === 'light' ? <Moon aria-hidden="true" /> : <Sun aria-hidden="true" />}</button><a href="https://github.com/paradigmxyz/solar">repository ↗</a></nav></header>
}

function SiteFooter() { return <footer>Measured by the in-repository runtime corpus.</footer> }

function Home() {
  const [index, setIndex] = useState<RunIndex | null>(null)
  const [error, setError] = useState('')
  const [base, setBase] = useState('')
  const [head, setHead] = useState('')
  const runs = useMemo(() => index?.runs ?? [], [index])
  const mainRuns = useMemo(() => runs.filter((run) => run.branch === 'main'), [runs])

  useEffect(() => { loadIndex().then(setIndex).catch((value: Error) => setError(value.message)) }, [])
  useEffect(() => {
    if (runs.length && !head) setHead(runs[0].commit)
    if (runs.length > 1 && !base) setBase(runs[1].commit)
  }, [base, head, runs])

  const compare = () => {
    if (!base || !head || base === head) return
    const url = new URL(window.location.href)
    url.search = new URLSearchParams({ base, head }).toString()
    window.location.href = url.toString()
  }

  return (
    <main className="dashboard">
        <section className="dashboard-title">
          <div><h1>Performance</h1><p>Main branch benchmark history</p></div>
          <span>{mainRuns.length} runs</span>
        </section>
        <section className="compare-box" aria-label="Compare commits">
          <label>base<select value={base} onChange={(event) => setBase(event.target.value)} disabled={!runs.length}><option value="">Select a run</option>{runs.map((run) => <option key={run.commit} value={run.commit}>{runLabel(run)}</option>)}</select></label>
          <span className="arrow">→</span>
          <label>head<select value={head} onChange={(event) => setHead(event.target.value)} disabled={!runs.length}><option value="">Select a run</option>{runs.map((run) => <option key={run.commit} value={run.commit}>{runLabel(run)}</option>)}</select></label>
          <button onClick={compare} disabled={!base || !head || base === head}>Compare</button>
        </section>
        {error ? <p className="error">{error}</p> : <section className="chart-grid">{charts.map((chart) => <HistoryGraph key={chart.metric} runs={runs} {...chart} />)}</section>}
        <section className="recent">
          <div className="section-heading"><h2>Recent runs</h2><span>lower is better</span></div>
          <div className="run run-head"><span>commit</span><span>ref</span><span>date</span><span>runtime gas</span><span>runtime bytes</span></div>
          {runs.length === 0 ? <p className="empty">No published benchmark runs yet.</p> : runs.slice(0, 12).map((run) => { const comparison = runs.find((candidate) => candidate.commit !== run.commit)?.commit; const contents = <><code>{short(run.commit)}</code><span>{runRef(run)}</span><time>{new Date(run.timestamp).toLocaleDateString()}</time><strong>{run.metrics.runtimeGas?.toLocaleString() ?? 'n/a'}</strong><strong>{run.metrics.runtimeSize?.toLocaleString() ?? 'n/a'}</strong></>; return comparison ? <a className="run" key={run.commit} href={`?base=${comparison}&head=${run.commit}`}>{contents}</a> : <div className="run" key={run.commit}>{contents}</div> })}
        </section>
    </main>
  )
}
