module.exports = async function validateSource({ github, repo, run, source, tagged }) {
  if (tagged !== source || run.path !== '.github/workflows/release.yml') {
    throw new Error('Release source or workflow does not match the verified build');
  }
  if (run.head_sha === source) return;
  const { data } = await github.rest.repos.compareCommitsWithBasehead({
    ...repo,
    basehead: `${source}...${run.head_sha}`,
  });
  const allowed = (file) => file.startsWith('.github/workflows/')
    || file.startsWith('.github/scripts/release/')
    || file === 'src-tauri/scripts/finalize-staged-windows.mjs';
  if (data.status !== 'ahead' || !Array.isArray(data.files)
    || data.files.length >= 300 || data.total_commits !== data.commits?.length
    || data.files.some((file) => !allowed(file.filename)
      || (file.previous_filename && !allowed(file.previous_filename)))) {
    throw new Error('Application source changed between release tag and build tooling');
  }
};
