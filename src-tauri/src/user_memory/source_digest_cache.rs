//! 来源摘要缓存：权威快照未变时复用已算好的来源摘要，
//! 避免每次召回都重新读取、解析并校验完整的权威快照。

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use crate::app_error::AppCommandError;

use super::authority_types::AuthoritySnapshot;
use super::index_parse::source_digest;
use super::{authority_sql as sql, UserMemoryLearningState, UserMemoryPolicy, UserMemoryService};

const ACTIVE_MODE: &str = "active";
const IDENTITY_SQL: &str =
    "SELECT store_id, mode, epoch, source_digest FROM memory_authority WHERE root_key = ?";

/// 唯一确定一份 active 权威快照数据的标识；`digest` 是快照数据的内容哈希。
#[derive(Clone, PartialEq, Eq)]
pub(super) struct AuthorityIdentity {
    store_id: String,
    epoch: i64,
    digest: String,
}

impl AuthorityIdentity {
    /// 仅 active 快照可缓存；其余模式的来源在磁盘文件上，没有可比较的标识。
    pub(super) fn of_active(snapshot: Option<&AuthoritySnapshot>) -> Option<Self> {
        snapshot
            .filter(|snapshot| snapshot.mode == ACTIVE_MODE)
            .map(|snapshot| Self {
                store_id: snapshot.store_id.clone(),
                epoch: snapshot.epoch,
                digest: snapshot.digest.clone(),
            })
    }
}

pub(super) struct CachedSourceDigest {
    identity: AuthorityIdentity,
    /// 未混入 managed chat root 的摘要；共享缓存的各实例在返回时自行 scope。
    unscoped: String,
    /// 摘要包含“已过期保留项”集合，只在 `[computed_at, valid_until)` 内成立：
    /// 下一个保留期到期或系统时钟回拨到计算时刻之前都必须重算。
    computed_at: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
}

pub(super) type SourceDigestCache = Arc<Mutex<Option<CachedSourceDigest>>>;

pub(super) struct ComputedSourceDigest {
    unscoped: String,
    computed_at: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
}

impl UserMemoryService {
    /// 免锁快速路径：命中时返回已 scope 的摘要；任何不确定的情况都返回 `None`，
    /// 由完整路径重新加载快照并给出准确的错误。
    pub(super) async fn cached_fast_source_digest(&self) -> Option<String> {
        let identity = self.stored_authority_identity().await.ok().flatten()?;
        let digest = self.cached_source_digest(&identity)?;
        self.marker_accepts(&identity).then_some(digest)
    }

    /// 已持写锁并加载权威快照后的路径：命中缓存则复用，否则计算并回填。
    pub(super) async fn loaded_source_digest(
        &self,
        identity: AuthorityIdentity,
    ) -> Result<String, AppCommandError> {
        if let Some(digest) = self.cached_source_digest(&identity) {
            return Ok(digest);
        }
        let policy = self.load_policy_unrecovered().await?;
        let computed = self.compute_source_digest(&policy)?;
        Ok(self.remember_source_digest(Some(identity), computed))
    }

    /// 读取内存中已加载快照的标识，不克隆快照正文。
    pub(super) fn loaded_authority_identity(&self) -> Option<AuthorityIdentity> {
        let guard = self
            .authority
            .read()
            .unwrap_or_else(|error| error.into_inner());
        AuthorityIdentity::of_active(guard.as_ref())
    }

    /// `now` 必须先于摘要计算取值：若有保留项恰在两者之间到期，
    /// 有效期会落在过去，缓存立即失效，而不是长期保留旧摘要。
    pub(super) fn compute_source_digest(
        &self,
        policy: &UserMemoryPolicy,
    ) -> Result<ComputedSourceDigest, AppCommandError> {
        let now = Utc::now();
        let settings = super::index_source::readonly_snapshot(self, policy)?;
        let candidates = self.read_learning_optional()?;
        Ok(ComputedSourceDigest {
            unscoped: source_digest(&settings, candidates.as_ref()),
            computed_at: now,
            valid_until: next_retention_expiry(candidates.as_ref(), now),
        })
    }

    /// 返回已 scope 的摘要；仅当来源是 active 权威快照时写入缓存。
    pub(super) fn remember_source_digest(
        &self,
        identity: Option<AuthorityIdentity>,
        computed: ComputedSourceDigest,
    ) -> String {
        let scoped = self.scoped_source_digest(&computed.unscoped);
        if let Some(identity) = identity {
            *self.lock_source_digest_cache() = Some(CachedSourceDigest {
                identity,
                unscoped: computed.unscoped,
                computed_at: computed.computed_at,
                valid_until: computed.valid_until,
            });
        }
        scoped
    }

    fn cached_source_digest(&self, identity: &AuthorityIdentity) -> Option<String> {
        let guard = self.lock_source_digest_cache();
        let cached = guard.as_ref()?;
        let now = Utc::now();
        let fresh = cached.computed_at <= now && cached.valid_until.is_none_or(|until| now < until);
        (fresh && cached.identity == *identity).then(|| self.scoped_source_digest(&cached.unscoped))
    }

    fn lock_source_digest_cache(&self) -> std::sync::MutexGuard<'_, Option<CachedSourceDigest>> {
        self.source_digest_cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// 只读取权威行的标识列，不加载快照正文。
    async fn stored_authority_identity(
        &self,
    ) -> Result<Option<AuthorityIdentity>, AppCommandError> {
        let key = self.authority_key()?;
        let rows = sql::rows(&self.db, IDENTITY_SQL, vec![key.into()]).await?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let mode: String = sql::field(row, "mode")?;
        if mode != ACTIVE_MODE {
            return Ok(None);
        }
        Ok(Some(AuthorityIdentity {
            store_id: sql::field(row, "store_id")?,
            epoch: sql::field(row, "epoch")?,
            digest: sql::field(row, "source_digest")?,
        }))
    }

    /// 与 `authority_export::validate_marker` 对 active 快照的判定一致。
    fn marker_accepts(&self, identity: &AuthorityIdentity) -> bool {
        let Ok(root) = self.resolved_root() else {
            return false;
        };
        matches!(
            super::authority_export::marker(root),
            Ok(Some(marker)) if marker.store_id == identity.store_id
                && identity.epoch >= marker.epoch
                && (identity.epoch > marker.epoch || identity.digest == marker.digest)
        )
    }
}

/// 下一个尚未到期的保留期时间点；无法解析的时间与 `source_digest` 一致视为永不过期。
fn next_retention_expiry(
    candidates: Option<&UserMemoryLearningState>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    candidates
        .into_iter()
        .flat_map(|state| state.retention.values())
        .filter_map(|record| DateTime::parse_from_rfc3339(&record.expires_at).ok())
        .map(|expires| expires.with_timezone(&Utc))
        .filter(|expires| *expires > now)
        .min()
}
