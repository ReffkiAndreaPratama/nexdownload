export type DownloadStatus =
  | "queued"
  | "preparing"
  | "connecting"
  | "downloading"
  | "pausing"
  | "paused"
  | "verifying"
  | "completed"
  | "failed"
  | "cancelled"
  | "waiting_for_network";

export interface DownloadSegment {
  index: number;
  start: number;
  end: number;
  downloaded: number;
}

export interface DownloadTask {
  id: string;
  url: string;
  final_url: string | null;
  file_name: string;
  destination_dir: string;
  file_path: string;
  category: string;
  queue_id: string | null;
  collection_id: string | null;
  status: DownloadStatus;
  priority: number;
  total_bytes: number | null;
  downloaded_bytes: number;
  connections: number;
  speed_bps: number;
  error: string | null;
  error_kind: string | null;
  etag: string | null;
  last_modified: string | null;
  accepts_ranges: boolean;
  content_type: string | null;
  checksum_sha256: string | null;
  expected_checksum: string | null;
  source_domain: string | null;
  created_at: string;
  updated_at: string;
  started_at: string | null;
  completed_at: string | null;
  segments: DownloadSegment[];
}

export interface Stats {
  total_tasks: number;
  active: number;
  queued: number;
  paused: number;
  completed: number;
  failed: number;
  cancelled: number;
  total_downloaded_bytes: number;
  total_known_bytes: number;
  current_speed_bps: number;
  peak_speed_bps: number;
  average_speed_bps: number;
  average_duration_secs: number;
  success_rate: number;
  completed_today: number;
}

export interface Category {
  id: string;
  name: string;
  extensions: string[];
  destination_dir: string | null;
  builtin: boolean;
}

export interface Queue {
  id: string;
  name: string;
  priority: number;
  max_concurrent: number;
  status: "active" | "paused" | string;
  created_at: string;
}

export interface Collection {
  id: string;
  name: string;
  description: string | null;
  color: string | null;
  created_at: string;
}

export interface RuleConditions {
  extensions: string[];
  domains: string[];
  filename_regex: string | null;
  min_size: number | null;
  max_size: number | null;
  content_types: string[];
  url_regex: string | null;
}

export interface RuleActions {
  category: string | null;
  destination_dir: string | null;
  queue_id: string | null;
  priority: number | null;
  connections: number | null;
  require_confirmation: boolean;
}

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  priority: number;
  conditions: RuleConditions;
  actions: RuleActions;
}

export interface StoredRule extends Omit<Rule, "conditions" | "actions"> {
  conditions: RuleConditions;
  actions: RuleActions;
  created_at: string;
}

export interface CategoryStat {
  category: string;
  count: number;
  bytes: number;
}

export interface DailyStat {
  date: string;
  completed: number;
  bytes: number;
}

export interface QueueStat {
  queue_id: string;
  name: string;
  active: number;
  queued: number;
  total: number;
}

export interface SpeedSample {
  sampled_at: string;
  speed_bps: number;
  downloaded_bytes: number;
}

export interface Analytics {
  stats: Stats;
  categories: CategoryStat[];
  daily: DailyStat[];
  queues: QueueStat[];
  speed_history: SpeedSample[];
}

export interface AddResult {
  task: DownloadTask;
  duplicate_of: string | null;
  rule_applied: string | null;
}

export interface NewDownload {
  url: string;
  file_name?: string | null;
  directory?: string | null;
  category?: string | null;
  queue_id?: string | null;
  collection_id?: string | null;
  connections?: number | null;
  priority?: number | null;
  expected_checksum?: string | null;
  confirmed?: boolean;
}

export interface Diagnostics {
  task_id: string;
  summary: string;
  error_kind: string | null;
  error: string | null;
  hints: string[];
  source_reachable: boolean | null;
  resource: {
    final_url: string;
    file_name: string | null;
    content_type: string | null;
    total_bytes: number | null;
    accepts_ranges: boolean;
    etag: string | null;
    last_modified: string | null;
    server: string | null;
  } | null;
  events: TaskEvent[];
  integrity: IntegrityRecord[];
  suggested_actions: string[];
}

export interface TaskEvent {
  id: number;
  task_id: string;
  from_state: string | null;
  to_state: string;
  note: string | null;
  created_at: string;
}

export interface IntegrityRecord {
  id: number;
  task_id: string;
  algorithm: string;
  expected: string | null;
  actual: string | null;
  status: string;
  expected_size: number | null;
  actual_size: number | null;
  checked_at: string;
}

export interface IntegrityReport {
  status: "verified" | "checksum_mismatch" | "size_mismatch" | "not_configured";
  algorithm: string;
  actual_sha256: string;
  expected_sha256: string | null;
  actual_size: number;
  expected_size: number | null;
  checked_at: string;
}

export interface BenchmarkResult {
  url: string;
  status: number;
  connect_and_headers_ms: number;
  download_ms: number;
  bytes: number;
  throughput_bps: number;
  dns_ms: number | null;
  tls_ms: number | null;
  note: string;
}

export interface DuplicateGroup {
  checksum: string;
  tasks: DownloadTask[];
}
