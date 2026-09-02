import { File, MultiFileDiff } from '@pierre/diffs/react'
import { useEffect, useState } from 'react'
import { loadArtifact } from './data'
import { artifactLanguage } from './highlight'
import type { Theme } from './types'

interface Props {
  before: { commit: string; benchmark: string; compiler: string }
  after: { commit: string; benchmark: string; compiler: string }
  path: string
  storagePath: string
  language: string
  theme: Theme
}

function formatContents(contents: string | null, language: string) {
  if (contents === null) return null
  if (language !== 'json') return contents
  try {
    return `${JSON.stringify(JSON.parse(contents), null, 2)}\n`
  } catch {
    return contents
  }
}

export default function ArtifactDiff({ before, after, path, storagePath, language, theme }: Props) {
  const [contents, setContents] = useState<[string | null, string | null] | null>(null)
  const [error, setError] = useState('')
  const [style, setStyle] = useState<'split' | 'unified'>('split')
  useEffect(() => {
    setContents(null)
    setError('')
    Promise.all([
      loadArtifact(before.commit, before.benchmark, before.compiler, storagePath),
      loadArtifact(after.commit, after.benchmark, after.compiler, storagePath),
    ]).then(([beforeContents, afterContents]) => setContents([
      formatContents(beforeContents, language),
      formatContents(afterContents, language),
    ])).catch((value: Error) => setError(value.message))
  }, [after.benchmark, after.commit, after.compiler, before.benchmark, before.commit, before.compiler, language, storagePath])
  if (error) return <p className="error">Could not load artifact: {error}</p>
  if (!contents) return <p className="empty">Loading diff…</p>
  const lang = artifactLanguage(path, language)
  const oldFile = contents[0] === null ? null : { name: path, contents: contents[0], lang }
  const newFile = contents[1] === null ? null : { name: path, contents: contents[1], lang }
  if (oldFile === null && newFile === null) {
    return <p className="empty">This artifact was not published by either side.</p>
  }
  if (!oldFile || !newFile || oldFile.contents === newFile.contents) {
    return <File className="solar-diff" file={oldFile ?? newFile!} options={{ overflow: 'scroll', themeType: theme }} disableWorkerPool />
  }
  return <div className="artifact-diff">
    <div className="diff-tools"><button className={style === 'split' ? 'active' : ''} onClick={() => setStyle('split')}>Split</button><button className={style === 'unified' ? 'active' : ''} onClick={() => setStyle('unified')}>Unified</button></div>
    {oldFile && newFile ? <MultiFileDiff className="solar-diff" oldFile={oldFile} newFile={newFile} options={{ diffStyle: style, overflow: 'scroll', themeType: theme }} disableWorkerPool /> : oldFile ? <MultiFileDiff className="solar-diff" oldFile={oldFile} newFile={null} options={{ diffStyle: style, overflow: 'scroll', themeType: theme }} disableWorkerPool /> : <MultiFileDiff className="solar-diff" oldFile={null} newFile={newFile!} options={{ diffStyle: style, overflow: 'scroll', themeType: theme }} disableWorkerPool />}
  </div>
}
