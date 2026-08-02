use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    config::{shared_gate, write_json_atomically},
    SettingsStore, UsageProfileId, WindowKind,
};

const SCHEMA_VERSION: u32 = 1;
const HISTORY_FILE: &str = "usage-history.json";
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SAMPLES_PER_STREAM: usize = 1_000;
const RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const MINIMUM_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// 저장할 수 없는 사용량 이력 표본의 원인을 나타냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageHistoryError {
    /// 시스템 프로필이 아니면서 번호가 0인 관리 프로필입니다.
    InvalidProfile,
    /// 사용량 비율이 음수이거나 유한한 값이 아닙니다.
    InvalidUsage,
    /// 표본 또는 초기화 시각이 UNIX epoch 이전입니다.
    PreEpochTimestamp,
    /// 관측 시각이 호출자가 제공한 현재 시각보다 미래입니다.
    FutureObservation,
    /// 초기화 시각 또는 같은 스트림의 관측 시각 순서가 거꾸로입니다.
    ReversedTimestamps,
}

/// 이력 표본 기록 요청의 처리 결과를 나타냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageHistoryRecord {
    /// 표본을 이력에 추가했습니다.
    Added,
    /// 동일한 표본이 이미 있어 추가하지 않았습니다.
    SkippedDuplicate,
    /// 같은 초기화 구간에서 최소 관측 간격보다 이른 표본을 추가하지 않았습니다.
    SkippedMinimumInterval,
    /// 30일 보존 범위를 벗어난 과거 표본을 추가하지 않았습니다.
    SkippedExpired,
}

impl UsageHistoryRecord {
    /// 표본이 실제 이력에 추가되었는지 반환합니다.
    pub fn is_added(self) -> bool {
        self == Self::Added
    }
}

/// 하나의 프로필·사용량 창에서 관측한 검증된 사용량 표본입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageSample {
    profile_id: UsageProfileId,
    window_kind: WindowKind,
    used_percent: f64,
    resets_at: Option<SystemTime>,
    observed_at: SystemTime,
}

impl UsageSample {
    /// 안전하게 저장할 수 있는 사용량 표본을 생성합니다.
    ///
    /// `used_percent`는 유한한 0 이상의 값이어야 합니다. 모든 시각은 UNIX epoch 이후여야 하며,
    /// `observed_at`은 `now` 이후일 수 없습니다. 초기화 시각이 있으면 관측 시각보다 이르면 안 됩니다.
    pub fn new(
        profile_id: UsageProfileId,
        window_kind: WindowKind,
        used_percent: f64,
        resets_at: Option<SystemTime>,
        observed_at: SystemTime,
        now: SystemTime,
    ) -> Result<Self, UsageHistoryError> {
        let sample = Self {
            profile_id,
            window_kind,
            used_percent,
            resets_at,
            observed_at,
        };
        sample.validate(now)?;
        Ok(sample)
    }

    /// 표본이 속한 내부 사용량 프로필 식별자를 반환합니다.
    pub fn profile_id(&self) -> UsageProfileId {
        self.profile_id
    }

    /// 표본이 속한 기본 또는 보조 사용량 창을 반환합니다.
    pub fn window_kind(&self) -> WindowKind {
        self.window_kind
    }

    /// 서버가 보고한 원본 사용량 비율을 반환합니다.
    pub fn used_percent(&self) -> f64 {
        self.used_percent
    }

    /// 서버가 보고한 다음 초기화 시각을 반환합니다.
    pub fn resets_at(&self) -> Option<SystemTime> {
        self.resets_at
    }

    /// 표본을 관측한 시각을 반환합니다.
    pub fn observed_at(&self) -> SystemTime {
        self.observed_at
    }

    fn validate(&self, now: SystemTime) -> Result<(), UsageHistoryError> {
        if self.profile_id == UsageProfileId::Managed(0) {
            return Err(UsageHistoryError::InvalidProfile);
        }
        if !self.used_percent.is_finite() || self.used_percent < 0.0 {
            return Err(UsageHistoryError::InvalidUsage);
        }
        if unix_seconds(self.observed_at).is_none()
            || self
                .resets_at
                .is_some_and(|reset| unix_seconds(reset).is_none())
        {
            return Err(UsageHistoryError::PreEpochTimestamp);
        }
        if self.observed_at > now {
            return Err(UsageHistoryError::FutureObservation);
        }
        if self.resets_at.is_some_and(|reset| reset < self.observed_at) {
            return Err(UsageHistoryError::ReversedTimestamps);
        }
        Ok(())
    }
}

/// 프로필과 사용량 창별로 제한된 표본을 보관하는 메모리 이력입니다.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageHistory {
    samples: Vec<UsageSample>,
}

impl UsageHistory {
    /// 전체 표본을 관측 시각 순서대로 반환합니다.
    pub fn samples(&self) -> &[UsageSample] {
        &self.samples
    }

    /// 지정한 프로필과 사용량 창에 속하는 표본만 순서대로 반환합니다.
    pub fn samples_for(
        &self,
        profile_id: UsageProfileId,
        window_kind: WindowKind,
    ) -> impl Iterator<Item = &UsageSample> {
        self.samples.iter().filter(move |sample| {
            sample.profile_id == profile_id && sample.window_kind == window_kind
        })
    }

    /// 검증된 표본을 기록하고 중복·최소 간격·보존 범위를 적용합니다.
    ///
    /// `now`는 보존 기간과 미래 시각을 결정합니다. 같은 프로필·창에서 관측 시각이 이전 표본보다
    /// 과거면 순서가 뒤집힌 것으로 거부합니다.
    pub fn record(
        &mut self,
        sample: UsageSample,
        now: SystemTime,
    ) -> Result<UsageHistoryRecord, UsageHistoryError> {
        sample.validate(now)?;
        self.prune(now);
        let cutoff = now.checked_sub(RETENTION).unwrap_or(UNIX_EPOCH);
        if sample.observed_at < cutoff {
            return Ok(UsageHistoryRecord::SkippedExpired);
        }
        let stream: Vec<&UsageSample> = self
            .samples_for(sample.profile_id, sample.window_kind)
            .collect();
        if stream.iter().any(|existing| **existing == sample) {
            return Ok(UsageHistoryRecord::SkippedDuplicate);
        }
        if let Some(last) = stream.last() {
            if sample.observed_at < last.observed_at {
                return Err(UsageHistoryError::ReversedTimestamps);
            }
            if sample.resets_at == last.resets_at
                && sample
                    .observed_at
                    .duration_since(last.observed_at)
                    .unwrap_or_default()
                    < MINIMUM_INTERVAL
            {
                return Ok(UsageHistoryRecord::SkippedMinimumInterval);
            }
        }
        self.samples.push(sample);
        self.prune(now);
        Ok(UsageHistoryRecord::Added)
    }

    /// 지정한 프로필에 속한 표본을 모두 제거하고 제거 개수를 반환합니다.
    pub fn remove_profile(&mut self, profile_id: UsageProfileId) -> usize {
        let before = self.samples.len();
        self.samples
            .retain(|sample| sample.profile_id != profile_id);
        before - self.samples.len()
    }

    /// 모든 표본을 제거합니다.
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    fn prune(&mut self, now: SystemTime) {
        let cutoff = now.checked_sub(RETENTION).unwrap_or(UNIX_EPOCH);
        self.samples.retain(|sample| sample.observed_at >= cutoff);
        self.samples.sort_by_key(|sample| sample.observed_at);
        let mut retained = Vec::with_capacity(self.samples.len());
        let mut stream_counts = HashMap::new();
        for sample in self.samples.drain(..).rev() {
            let window_key = match sample.window_kind {
                WindowKind::Primary => 0_u8,
                WindowKind::Secondary => 1_u8,
            };
            let count = stream_counts
                .entry((sample.profile_id, window_key))
                .or_insert(0);
            if *count < MAX_SAMPLES_PER_STREAM {
                retained.push(sample);
                *count += 1;
            }
        }
        retained.reverse();
        self.samples = retained;
    }
}

/// 사용량 이력 JSON 파일을 안전하게 읽고 쓰는 저장소입니다.
#[derive(Clone, Debug)]
pub struct UsageHistoryStore {
    root: PathBuf,
    gate: Arc<Mutex<()>>,
}

impl UsageHistoryStore {
    /// 기본 CodexPeek 앱 데이터 경로를 사용하는 저장소를 생성합니다.
    pub fn new() -> Self {
        Self::for_root(SettingsStore::new().root().to_path_buf())
    }

    /// 테스트 또는 이식 가능한 실행을 위해 지정 경로 아래 저장소를 생성합니다.
    ///
    /// 생성 시에는 디렉터리나 파일을 만들지 않으며, 저장 파일은 `root/usage-history.json`입니다.
    pub fn for_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            gate: shared_gate(&root),
            root,
        }
    }

    /// 이력 파일을 보관하는 루트 경로를 반환합니다.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 이력 JSON 파일의 전체 경로를 반환합니다.
    pub fn path(&self) -> PathBuf {
        self.root.join(HISTORY_FILE)
    }

    /// 이력을 읽고 손상되었거나 지원하지 않는 파일은 격리한 뒤 빈 이력을 반환합니다.
    ///
    /// 파일이 없으면 파일 시스템을 변경하지 않고 빈 이력을 반환합니다. 읽기·격리 I/O 실패는 호출자에게
    /// 전달합니다. `now`는 파일 표본의 미래 시각 검사와 보존 범위에 사용됩니다.
    pub fn load(&self, now: SystemTime) -> io::Result<UsageHistory> {
        let _gate = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        let path = self.path();
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(UsageHistory::default())
            }
            Err(error) => return Err(error),
        };
        if metadata.len() > MAX_FILE_BYTES {
            self.quarantine(&path, now)?;
            return Ok(UsageHistory::default());
        }
        let bytes = fs::read(&path)?;
        let Some(mut history) = decode_history(&bytes, now) else {
            self.quarantine(&path, now)?;
            return Ok(UsageHistory::default());
        };
        history.prune(now);
        Ok(history)
    }

    /// 이력을 원자적으로 저장합니다.
    ///
    /// 저장 전 `now` 기준으로 만료 표본과 스트림별 초과 표본을 제거합니다. 파일 I/O 실패는 호출자에게
    /// 반환되며 기존 대상 파일은 원자 교체가 성공할 때까지 유지됩니다.
    pub fn save(&self, history: &UsageHistory, now: SystemTime) -> io::Result<()> {
        let _gate = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        let mut bounded = history.clone();
        bounded.prune(now);
        let bytes = encode_history(&bounded)?;
        write_json_atomically(&self.path(), ".usage-history.tmp", &bytes)
    }

    fn quarantine(&self, path: &Path, now: SystemTime) -> io::Result<()> {
        let stamp = unix_seconds(now).unwrap_or_default();
        let backup = self.root.join(format!(
            "usage-history.corrupt-{stamp}-{}-{}.json",
            std::process::id(),
            next_nonce()
        ));
        fs::rename(path, backup)
    }
}

impl Default for UsageHistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize, Serialize)]
struct HistoryEnvelope {
    schema_version: u32,
    samples: Vec<StoredSample>,
}

#[derive(Deserialize, Serialize)]
struct StoredSample {
    profile_id: String,
    window_kind: String,
    used_percent: f64,
    resets_at: Option<u64>,
    observed_at: u64,
}

fn decode_history(bytes: &[u8], now: SystemTime) -> Option<UsageHistory> {
    if bytes.is_empty() {
        return None;
    }
    let envelope: HistoryEnvelope = serde_json::from_slice(bytes).ok()?;
    if envelope.schema_version != SCHEMA_VERSION {
        return None;
    }
    let mut history = UsageHistory::default();
    for stored in envelope.samples {
        let sample = UsageSample::new(
            parse_profile_id(&stored.profile_id)?,
            parse_window_kind(&stored.window_kind)?,
            stored.used_percent,
            stored.resets_at.map(unix_time)?,
            unix_time(stored.observed_at)?,
            now,
        )
        .ok()?;
        history.record(sample, now).ok()?;
    }
    Some(history)
}

fn encode_history(history: &UsageHistory) -> io::Result<Vec<u8>> {
    let samples: Option<Vec<StoredSample>> = history
        .samples
        .iter()
        .map(|sample| {
            Some(StoredSample {
                profile_id: format_profile_id(sample.profile_id),
                window_kind: format_window_kind(sample.window_kind).to_owned(),
                used_percent: sample.used_percent,
                resets_at: sample.resets_at.and_then(unix_seconds),
                observed_at: unix_seconds(sample.observed_at)?,
            })
        })
        .collect();
    serde_json::to_vec_pretty(&HistoryEnvelope {
        schema_version: SCHEMA_VERSION,
        samples: samples.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid history timestamp")
        })?,
    })
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn format_profile_id(profile_id: UsageProfileId) -> String {
    match profile_id {
        UsageProfileId::System => "system".to_owned(),
        UsageProfileId::Managed(sequence) => format!("managed:{sequence}"),
    }
}

fn parse_profile_id(value: &str) -> Option<UsageProfileId> {
    if value == "system" {
        return Some(UsageProfileId::System);
    }
    value
        .strip_prefix("managed:")?
        .parse::<u32>()
        .ok()
        .filter(|sequence| *sequence != 0)
        .map(UsageProfileId::Managed)
}

fn format_window_kind(window_kind: WindowKind) -> &'static str {
    match window_kind {
        WindowKind::Primary => "primary",
        WindowKind::Secondary => "secondary",
    }
}

fn parse_window_kind(value: &str) -> Option<WindowKind> {
    match value {
        "primary" => Some(WindowKind::Primary),
        "secondary" => Some(WindowKind::Secondary),
        _ => None,
    }
}

fn unix_seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn unix_time(seconds: u64) -> Option<SystemTime> {
    UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

fn next_nonce() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NONCE: AtomicU64 = AtomicU64::new(0);
    NONCE.fetch_add(1, Ordering::Relaxed)
}
