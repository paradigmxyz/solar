import { useEffect, useMemo, useState } from 'react'
import { Moon, Sun } from 'lucide-react'
import { changeClass, formatChange } from './change'
import { loadIndex } from './data'
import type { MetricSummary, RunIndex, RunSummary, Theme } from './types'
import { Compare } from './Compare'
import { FileViewer } from './FileViewer'

const short = (commit: string) => commit.slice(0, 8)

const charts: { metric: keyof MetricSummary; title: string; unit: string }[] = [
  { metric: 'runtimeGas', title: 'Runtime gas', unit: 'gas' },
  { metric: 'deployGas', title: 'Deployment gas', unit: 'gas' },
  { metric: 'runtimeSize', title: 'Runtime bytecode', unit: 'bytes' },
  { metric: 'creationSize', title: 'Creation bytecode', unit: 'bytes' },
  { metric: 'compileTime', title: 'Compile time', unit: 'seconds' },
  { metric: 'peakMemory', title: 'Peak memory (RSS)', unit: 'memory' },
]

function formatValue(value: number, unit: string) {
  if (unit === 'seconds') return `${value.toFixed(2)} s`
  if (unit === 'memory')
    return value >= 1024 * 1024
      ? `${(value / 1024 / 1024).toFixed(1)} MiB`
      : `${Math.round(value / 1024).toLocaleString()} KiB`
  return `${Math.round(value).toLocaleString()} ${unit}`
}

function runRef(run: RunSummary) {
  return run.branch ?? (run.pr ? `PR #${run.pr}` : 'detached')
}

function runTitle(run: RunSummary) {
  return run.title || runRef(run)
}

function runLabel(run: RunSummary) {
  return `${short(run.commit)} · ${runTitle(run)} · ${new Date(run.timestamp).toLocaleDateString()}`
}

function resolveCommit(value: string, runs: RunSummary[]) {
  const normalized = value.trim().toLowerCase()
  return (
    runs.find((run) => run.commit === normalized)?.commit ??
    (normalized.length >= 7
      ? runs.find((run) => run.commit.startsWith(normalized))?.commit
      : undefined) ??
    ''
  )
}

function CommitPicker({
  label,
  value,
  runs,
  onChange,
}: {
  label: string
  value: string
  runs: RunSummary[]
  onChange: (value: string) => void
}) {
  const list = `${label}-runs`
  return (
    <label>
      {label}
      <input
        list={list}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder="Commit SHA"
        disabled={!runs.length}
        spellCheck={false}
        aria-label={`${label} commit`}
      />
      <datalist id={list}>
        {runs.map((run) => (
          <option key={run.commit} value={run.commit} label={runLabel(run)} />
        ))}
      </datalist>
    </label>
  )
}

function HistoryGraph({
  runs,
  metric,
  title,
  unit,
}: {
  runs: RunSummary[]
  metric: keyof MetricSummary
  title: string
  unit: string
}) {
  const [hovered, setHovered] = useState<number | null>(null)
  const points = runs
    .filter((run) => run.branch === 'main' && typeof run.metrics[metric] === 'number')
    .slice(0, 60)
    .reverse()
  const values = points.map((run) => run.metrics[metric]!)
  const min = Math.min(...values)
  const max = Math.max(...values)
  const range = max - min
  const padding = range === 0 ? Math.max(Math.abs(max) * 0.04, 1) : range * 0.1
  const chartMin = min - padding
  const chartMax = max + padding
  const position = (value: number) =>
    Math.max(10, Math.min(90, 90 - ((value - chartMin) / (chartMax - chartMin)) * 80))
  const path = points
    .map((run, index) => {
      const x = 3 + (index / Math.max(points.length - 1, 1)) * 94
      const y = position(run.metrics[metric]!)
      return `${index ? 'L' : 'M'} ${x} ${y}`
    })
    .join(' ')
  const first = values[0]
  const latest = values.at(-1)
  const active = points[hovered ?? points.length - 1]
  const activeIndex = hovered ?? points.length - 1
  const activeX = 3 + (activeIndex / Math.max(points.length - 1, 1)) * 94
  const activeY = active ? position(active.metrics[metric]!) : 0
  const change = first && latest !== undefined ? ((latest - first) / first) * 100 : null
  return (
    <section className="graph-card">
      <div className="graph-heading">
        <h2>{title}</h2>
        {active && latest !== undefined && (
          <div>
            <strong>{formatValue(active.metrics[metric]!, unit)}</strong>
            <span className={changeClass(change, false)}>{formatChange(change)}</span>
          </div>
        )}
      </div>
      {points.length < 2 ? (
        <div className="empty-graph">Waiting for two main-branch runs.</div>
      ) : (
        <>
          <div className="chart-body">
            <div className="chart-scale">
              <span>{formatValue(max, unit)}</span>
              <span>{formatValue(min, unit)}</span>
            </div>
            <div
              className="history-plot"
              onPointerMove={(event) => {
                const bounds = event.currentTarget.getBoundingClientRect()
                const x = (event.clientX - bounds.left) / bounds.width
                setHovered(
                  Math.max(
                    0,
                    Math.min(
                      points.length - 1,
                      Math.round(((x - 0.03) / 0.94) * (points.length - 1)),
                    ),
                  ),
                )
              }}
              onPointerLeave={() => setHovered(null)}
            >
              <svg
                className="history"
                viewBox="0 0 100 100"
                preserveAspectRatio="none"
                role="img"
                aria-label={`${title} over time`}
              >
                <path className="grid" d="M0 12H100 M0 50H100 M0 88H100" />
                <path className="series" d={path} />
              </svg>
              {hovered !== null && (
                <>
                  <span
                    className="chart-crosshair chart-crosshair-x"
                    style={{ left: `${activeX}%` }}
                  />
                  <span
                    className="chart-crosshair chart-crosshair-y"
                    style={{ top: `${activeY}%` }}
                  />
                  <span
                    className={`chart-tooltip${activeX > 65 ? ' tooltip-left' : ''}${activeY < 32 ? ' tooltip-below' : ''}`}
                    style={{ left: `${activeX}%`, top: `${activeY}%` }}
                  >
                    {new Date(active.timestamp).toLocaleString()} · {short(active.commit)} ·{' '}
                    {formatValue(active.metrics[metric]!, unit)}
                  </span>
                </>
              )}
              {points.map((run, index) => {
                const x = 3 + (index / Math.max(points.length - 1, 1)) * 94
                const y = position(run.metrics[metric]!)
                const label = `${formatValue(run.metrics[metric]!, unit)} · ${short(run.commit)} · ${new Date(run.timestamp).toLocaleDateString()}`
                return (
                  <button
                    key={run.commit}
                    className={`history-point${index === (hovered ?? points.length - 1) ? ' active-point' : ''}`}
                    style={{ left: `${x}%`, top: `${y}%` }}
                    onPointerEnter={() => setHovered(index)}
                    onPointerLeave={() => setHovered(null)}
                    onFocus={() => setHovered(index)}
                    onBlur={() => setHovered(null)}
                    onClick={() => {
                      const base = points[index - 1]
                      if (!base) return
                      const url = new URL(window.location.href)
                      url.search = new URLSearchParams({
                        base: base.commit,
                        head: run.commit,
                      }).toString()
                      window.location.href = url.toString()
                    }}
                    disabled={index === 0}
                    aria-label={label}
                    title={label}
                  />
                )
              })}
            </div>
          </div>
          <div className="chart-dates">
            <span>{new Date(points[0].timestamp).toLocaleDateString()}</span>
            <span>{new Date(points.at(-1)!.timestamp).toLocaleDateString()}</span>
          </div>
        </>
      )}
    </section>
  )
}

export function App() {
  const [theme, setTheme] = useState<Theme>(() =>
    document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light',
  )
  useEffect(() => {
    document.documentElement.dataset.theme = theme
  }, [theme])
  const route = new URLSearchParams(window.location.search)
  const baseCommit = route.get('base')
  const headCommit = route.get('head')
  const fileViewer = route.get('view') === 'files' && route.get('benchmark')
  const content =
    baseCommit && headCommit && baseCommit !== headCommit ? (
      fileViewer ? (
        <FileViewer
          base={baseCommit}
          head={headCommit}
          benchmark={route.get('benchmark')!}
          theme={theme}
        />
      ) : (
        <Compare base={baseCommit} head={headCommit} theme={theme} />
      )
    ) : (
      <Home />
    )
  const toggleTheme = () =>
    setTheme((value) => {
      const next = value === 'light' ? 'dark' : 'light'
      localStorage.setItem('solar-perf-theme', next)
      return next
    })
  return (
    <>
      <SiteHeader compact={Boolean(fileViewer)} theme={theme} onToggleTheme={toggleTheme} />
      {content}
      {!fileViewer && <SiteFooter />}
    </>
  )
}

function SiteHeader({
  compact,
  theme,
  onToggleTheme,
}: {
  compact: boolean
  theme: Theme
  onToggleTheme: () => void
}) {
  const nextTheme = theme === 'light' ? 'dark' : 'light'
  return (
    <header className={compact ? 'file-header' : ''}>
      <a className="wordmark" href={import.meta.env.BASE_URL}>
        solar<span>Performance</span>
      </a>
      <nav>
        {compact ? (
          <a href={import.meta.env.BASE_URL}>Overview</a>
        ) : (
          <>
            <a className="nav-active" href={import.meta.env.BASE_URL}>
              Dashboard
            </a>
            <a href="https://github.com/paradigmxyz/solar">Repository</a>
          </>
        )}
        <button
          className="theme-toggle"
          onClick={onToggleTheme}
          aria-label={`Switch to ${nextTheme} theme`}
          title={`Switch to ${nextTheme} theme`}
        >
          {theme === 'light' ? <Moon aria-hidden="true" /> : <Sun aria-hidden="true" />}
        </button>
      </nav>
    </header>
  )
}

function SiteFooter() {
  return <footer>Measured by the in-repository runtime corpus.</footer>
}

function Home() {
  const [index, setIndex] = useState<RunIndex | null>(null)
  const [error, setError] = useState('')
  const [base, setBase] = useState('')
  const [head, setHead] = useState('')
  const runs = useMemo(() => index?.runs ?? [], [index])
  const mainRuns = useMemo(() => runs.filter((run) => run.branch === 'main'), [runs])
  const selectedBase = resolveCommit(base, runs)
  const selectedHead = resolveCommit(head, runs)

  useEffect(() => {
    loadIndex()
      .then(setIndex)
      .catch((value: Error) => setError(value.message))
  }, [])
  useEffect(() => {
    if (runs.length && !head) setHead(runs[0].commit)
    if (runs.length > 1 && !base) setBase(runs[1].commit)
  }, [base, head, runs])

  const compare = () => {
    if (!selectedBase || !selectedHead || selectedBase === selectedHead) return
    const url = new URL(window.location.href)
    url.search = new URLSearchParams({ base: selectedBase, head: selectedHead }).toString()
    window.location.href = url.toString()
  }

  return (
    <main className="dashboard">
      <section className="dashboard-title">
        <div>
          <h1>Performance</h1>
          <p>Main branch benchmark history</p>
        </div>
        <span>{mainRuns.length} runs</span>
      </section>
      <section className="compare-box" aria-label="Compare commits">
        <CommitPicker label="base" value={base} runs={runs} onChange={setBase} />
        <span className="arrow">→</span>
        <CommitPicker label="head" value={head} runs={runs} onChange={setHead} />
        <button
          onClick={compare}
          disabled={!selectedBase || !selectedHead || selectedBase === selectedHead}
        >
          Compare
        </button>
      </section>
      {error ? (
        <p className="error">{error}</p>
      ) : (
        <section className="chart-grid">
          {charts.map((chart) => (
            <HistoryGraph key={chart.metric} runs={runs} {...chart} />
          ))}
        </section>
      )}
      <section className="recent">
        <div className="section-heading">
          <h2>Recent runs</h2>
          <span>lower is better</span>
        </div>
        <div className="run run-head">
          <span>commit</span>
          <span>change</span>
          <span>date</span>
          <span>runtime gas</span>
          <span>runtime bytes</span>
        </div>
        {runs.length === 0 ? (
          <p className="empty">No published benchmark runs yet.</p>
        ) : (
          runs.slice(0, 12).map((run) => {
            const comparison = runs.find((candidate) => candidate.commit !== run.commit)?.commit
            const contents = (
              <>
                <code>{short(run.commit)}</code>
                <span title={runTitle(run)}>{runTitle(run)}</span>
                <time>{new Date(run.timestamp).toLocaleDateString()}</time>
                <strong>{run.metrics.runtimeGas?.toLocaleString() ?? 'n/a'}</strong>
                <strong>{run.metrics.runtimeSize?.toLocaleString() ?? 'n/a'}</strong>
              </>
            )
            return comparison ? (
              <a className="run" key={run.commit} href={`?base=${comparison}&head=${run.commit}`}>
                {contents}
              </a>
            ) : (
              <div className="run" key={run.commit}>
                {contents}
              </div>
            )
          })
        )}
      </section>
    </main>
  )
}
