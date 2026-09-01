import { MultiFileDiff } from '@pierre/diffs/react'
import { useEffect, useState } from 'react'
import { loadArtifact } from './data'
import type { Theme } from './types'

interface Props {
  before: { commit: string; benchmark: string; compiler: string }
  after: { commit: string; benchmark: string; compiler: string }
  path: string
  storagePath: string
  language: string
  theme: Theme
}

export default function ArtifactDiff({ before, after, path, storagePath, language, theme }: Props) {
  const [contents, setContents] = useState<[string, string] | null>(null)
  const [error, setError] = useState('')
  const [style, setStyle] = useState<'split' | 'unified'>('split')
  useEffect(() => {
    setContents(null)
    setError('')
    Promise.all([
      loadArtifact(before.commit, before.benchmark, before.compiler, storagePath),
      loadArtifact(after.commit, after.benchmark, after.compiler, storagePath),
    ]).then(setContents).catch((value: Error) => setError(value.message))
  }, [after.benchmark, after.commit, after.compiler, before.benchmark, before.commit, before.compiler, storagePath])
  if (error) return <p className="error">Could not load artifact: {error}</p>
  if (!contents) return <p className="empty">Loading diff…</p>
  return <div className="artifact-diff">
    <div className="diff-tools"><button className={style === 'split' ? 'active' : ''} onClick={() => setStyle('split')}>Split</button><button className={style === 'unified' ? 'active' : ''} onClick={() => setStyle('unified')}>Unified</button></div>
    <MultiFileDiff className="solar-diff" oldFile={{ name: path, contents: contents[0], lang: language }} newFile={{ name: path, contents: contents[1], lang: language }} options={{ diffStyle: style, overflow: 'scroll', themeType: theme }} />
  </div>
}
