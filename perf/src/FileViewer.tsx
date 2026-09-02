import { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import { parseDiffFromFile } from '@pierre/diffs'
import { loadArtifact, loadRun } from './data'
import { artifactLanguage } from './highlight'
import type { ArtifactFile, RunDocument, Theme } from './types'

const ArtifactDiff = lazy(() => import('./ArtifactDiff'))

interface Props {
  base: string
  head: string
  benchmark: string
  theme: Theme
}

interface Counts {
  additions: number
  deletions: number
}

function fileCounts(oldContents: string | null, newContents: string | null, file: ArtifactFile): Counts {
  if (oldContents === null && newContents === null) return { additions: 0, deletions: 0 }
  const lang = artifactLanguage(file.path, file.language)
  const diff = parseDiffFromFile(
    oldContents === null ? null : { name: file.path, contents: oldContents, lang },
    newContents === null ? null : { name: file.path, contents: newContents, lang },
  )
  return diff.hunks.reduce((counts, hunk) => ({ additions: counts.additions + hunk.additionLines, deletions: counts.deletions + hunk.deletionLines }), { additions: 0, deletions: 0 })
}

export function FileViewer({ base, head, benchmark, theme }: Props) {
  const params = new URLSearchParams(window.location.search)
  const [runs, setRuns] = useState<[RunDocument, RunDocument] | null>(null)
  const [against, setAgainst] = useState<'base' | 'solc'>(params.get('against') === 'solc' ? 'solc' : 'base')
  const [selected, setSelected] = useState(params.get('file') ?? '')
  const [counts, setCounts] = useState<Record<string, Counts>>({})

  useEffect(() => { Promise.all([loadRun(base), loadRun(head)]).then(setRuns).catch(() => setRuns(null)) }, [base, head])
  const files = useMemo(() => runs ? runs[1].artifacts[benchmark] ?? runs[0].artifacts[benchmark] ?? [] : [], [benchmark, runs])
  const comparisonCommit = against === 'base' ? base : head
  const comparisonCompiler = against === 'base' ? 'solar' : 'solc'
  const selectedFile = files.find((file) => file.path === selected) ?? files[0]

  useEffect(() => {
    if (!files.length) return
    if (!selectedFile) setSelected(files[0].path)
    Promise.all(files.map(async (file) => [file.path, fileCounts(
      await loadArtifact(comparisonCommit, benchmark, comparisonCompiler, file.storagePath),
      await loadArtifact(head, benchmark, 'solar', file.storagePath),
      file,
    )] as const)).then((entries) => setCounts(Object.fromEntries(entries)))
  }, [benchmark, comparisonCommit, comparisonCompiler, files, head, selectedFile])

  const selectFile = (path: string) => {
    setSelected(path)
    const url = new URL(window.location.href)
    url.searchParams.set('file', path)
    history.replaceState(null, '', url)
  }
  const setComparison = (value: 'base' | 'solc') => {
    setAgainst(value)
    const url = new URL(window.location.href)
    url.searchParams.set('against', value)
    history.replaceState(null, '', url)
  }
  return <main className="file-viewer">{!runs ? <p className="empty">Loading files…</p> : !files.length ? <p className="empty">No files were published for this benchmark run.</p> : <div className="file-viewer-body"><aside><div className="file-selector-head"><span>{benchmark}</span><div className="toggle"><button className={against === 'base' ? 'active' : ''} onClick={() => setComparison('base')}>vs base</button><button className={against === 'solc' ? 'active' : ''} onClick={() => setComparison('solc')}>vs solc</button></div></div>{files.map((file) => { const count = counts[file.path]; return <button key={file.path} className={selectedFile?.path === file.path ? 'active' : ''} onClick={() => selectFile(file.path)}><span>{file.path}</span><small><i className="removed">−{count?.deletions ?? 0}</i><i className="added">+{count?.additions ?? 0}</i></small></button> })}</aside><div className="file-diff">{selectedFile && <Suspense fallback={<p className="empty">Loading renderer…</p>}><ArtifactDiff before={{ commit: comparisonCommit, benchmark, compiler: comparisonCompiler }} after={{ commit: head, benchmark, compiler: 'solar' }} path={selectedFile.path} storagePath={selectedFile.storagePath} language={selectedFile.language} theme={theme} /></Suspense>}</div></div>}</main>
}
