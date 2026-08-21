const { randomUUID } = require("node:crypto")

async function listReleaseAssets({ github, owner, repo, releaseId }) {
  return github.paginate(github.rest.repos.listReleaseAssets, {
    owner,
    repo,
    release_id: releaseId,
    per_page: 100,
  })
}

async function getAsset({ github, owner, repo, assetId }) {
  try {
    const response = await github.rest.repos.getReleaseAsset({
      owner,
      repo,
      asset_id: assetId,
    })
    return response.data
  } catch (error) {
    if (error?.status === 404) {
      return undefined
    }
    throw error
  }
}

async function uploadAsset({ github, owner, repo, releaseId, name, data }) {
  const response = await github.rest.repos.uploadReleaseAsset({
    owner,
    repo,
    release_id: releaseId,
    name,
    data,
    headers: {
      "content-type": "application/json",
      "content-length": Buffer.byteLength(data),
    },
  })
  return response.data
}

async function renameAsset({ github, owner, repo, assetId, name }) {
  try {
    const response = await github.rest.repos.updateReleaseAsset({
      owner,
      repo,
      asset_id: assetId,
      name,
    })
    return response.data
  } catch (error) {
    try {
      const current = await getAsset({ github, owner, repo, assetId })
      if (current?.name === name) {
        return current
      }
    } catch (verificationError) {
      throw new Error(
        `Asset ${assetId} rename could not be verified: ${verificationError.message}`,
        { cause: error }
      )
    }
    throw error
  }
}

function unknownDeletionError(error, verificationError, assetId) {
  const unknown = new Error(
    `Asset ${assetId} deletion could not be verified: ${verificationError.message}`,
    { cause: error }
  )
  unknown.assetStateUnknown = true
  return unknown
}

async function deleteAsset({ github, owner, repo, assetId }) {
  try {
    await github.rest.repos.deleteReleaseAsset({
      owner,
      repo,
      asset_id: assetId,
    })
  } catch (error) {
    if (error?.status === 404) {
      return
    }
    try {
      if (!(await getAsset({ github, owner, repo, assetId }))) {
        return
      }
    } catch (verificationError) {
      throw unknownDeletionError(error, verificationError, assetId)
    }
    throw error
  }
}

async function assertDraftRelease({ github, owner, repo, releaseId, name }) {
  const response = await github.rest.repos.getRelease({
    owner,
    repo,
    release_id: releaseId,
  })
  if (response.data?.draft !== true) {
    throw new Error(`Refusing to replace ${name} on a non-draft release`)
  }
}

function assertNoReplacementResidue(assets, name) {
  const prefixes = [`${name}.candidate-`, `${name}.backup-`]
  const residue = assets
    .map((asset) => asset.name)
    .filter((assetName) =>
      prefixes.some((prefix) => assetName.startsWith(prefix))
    )
  if (residue.length > 0) {
    throw new Error(
      `Temporary ${name} release assets are forbidden: ${residue.join(", ")}`
    )
  }
}

function replacementError(error, rollbackErrors) {
  if (rollbackErrors.length === 0) {
    return error
  }
  const details = rollbackErrors.map((item) => item.message).join("; ")
  return new Error(
    `${error.message}; release asset rollback failed: ${details}`,
    {
      cause: error,
    }
  )
}

async function collectCandidates(context, state, errors) {
  const candidates = new Map()
  if (state.candidate?.id != null) {
    candidates.set(state.candidate.id, state.candidate)
  }
  try {
    const assets = await listReleaseAssets(context)
    for (const asset of assets) {
      if (asset.name === state.candidateName && asset.id != null) {
        candidates.set(asset.id, asset)
      }
    }
  } catch (error) {
    errors.push(new Error(`candidate discovery failed: ${error.message}`))
  }
  return [...candidates.values()]
}

async function restoreExisting(context, errors) {
  if (!context.existing) {
    return
  }
  try {
    const previous = await getAsset({
      ...context,
      assetId: context.existing.id,
    })
    if (!previous) {
      throw new Error(`previous ${context.name} asset is missing`)
    }
    if (previous.name !== context.name) {
      await renameAsset({
        ...context,
        assetId: context.existing.id,
        name: context.name,
      })
    }
  } catch (error) {
    errors.push(
      new Error(`previous ${context.name} restore failed: ${error.message}`)
    )
  }
}

async function rollbackReplacement(context, state) {
  const errors = []
  const candidates = await collectCandidates(context, state, errors)
  for (const candidate of candidates) {
    try {
      await deleteAsset({ ...context, assetId: candidate.id })
    } catch (error) {
      errors.push(new Error(`candidate cleanup failed: ${error.message}`))
    }
  }
  await restoreExisting(context, errors)
  return errors
}

async function performReplacement(context, state) {
  state.candidate = await uploadAsset({
    ...context,
    name: state.candidateName,
  })
  context.assertReady(state.candidate, `Uploaded ${state.candidateName}`)
  if (context.existing) {
    const backup = await renameAsset({
      ...context,
      assetId: context.existing.id,
      name: state.backupName,
    })
    context.assertReady(backup, `Backed up ${context.name}`)
  }
  const promoted = await renameAsset({
    ...context,
    assetId: state.candidate.id,
    name: context.name,
  })
  context.assertReady(promoted, `Promoted ${context.name}`)
  await context.verifyReplacement(state.candidate.id)
  state.replacementVerified = true
  if (context.existing) {
    await deleteAsset({ ...context, assetId: context.existing.id })
  }
}

async function replaceReleaseAsset(context) {
  await assertDraftRelease(context)
  const suffix = randomUUID()
  const state = {
    candidateName: `${context.name}.candidate-${suffix}`,
    backupName: `${context.name}.backup-${suffix}`,
    candidate: undefined,
    replacementVerified: false,
  }
  try {
    await performReplacement(context, state)
  } catch (error) {
    if (state.replacementVerified && error.assetStateUnknown) {
      throw new Error(
        `${error.message}; verified ${context.name} was preserved`,
        { cause: error }
      )
    }
    const rollbackErrors = await rollbackReplacement(context, state)
    throw replacementError(error, rollbackErrors)
  }
  return Boolean(context.existing)
}

replaceReleaseAsset.assertNoReplacementResidue = assertNoReplacementResidue
module.exports = replaceReleaseAsset
