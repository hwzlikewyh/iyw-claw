module.exports = async function validateSource({ github, repo, run, source, tagged }) {
  if (tagged !== source || run.path !== '.github/workflows/release.yml') {
    throw new Error('Release source or workflow does not match the verified build');
  }
  if (run.head_sha === source) return;
  if (run.event === 'workflow_dispatch') {
    const { data: prepared } = await github.rest.repos.getCommit({ ...repo, ref: source });
    const versionFiles = new Set(['package.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock',
      'src-tauri/tauri.conf.json', 'src-tauri/tauri.windows.conf.json', 'src-tauri/tauri.windows-x86.conf.json']);
    const versionOnly = (file) => {
      if (!versionFiles.has(file.filename) || file.status !== 'modified' || !file.patch) return false;
      const changes = file.patch.split('\n').filter(line => /^[+-]/.test(line));
      return changes.length === 2 && changes.some(line => line[0] === '-') && changes.some(line => line[0] === '+') &&
        changes.every(line => /^[+-]\s*(?:"version":\s*"[^"]+",?|version\s*=\s*"[^"]+")\s*$/.test(line));
    };
    if (prepared.parents?.length === 1 && prepared.parents[0].sha === run.head_sha &&
        prepared.files?.length > 0 && prepared.files.every(versionOnly)) return;
  }
  const { data } = await github.rest.repos.compareCommitsWithBasehead({
    ...repo,
    basehead: `${source}...${run.head_sha}`,
  });
  const allowed = (file) => file.startsWith('.github/workflows/')
    || file.startsWith('.github/scripts/release/')
    || file === 'docs/release-desktop.md'
    || file === 'src-tauri/scripts/finalize-staged-windows.mjs';
  if (data.status !== 'ahead' || !Array.isArray(data.files)
    || data.files.length >= 300 || data.total_commits !== data.commits?.length
    || data.files.some((file) => !allowed(file.filename)
      || (file.previous_filename && !allowed(file.previous_filename)))) {
    throw new Error('Application source changed between release tag and build tooling');
  }
};
