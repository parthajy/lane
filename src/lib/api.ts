import { invoke as tauriInvoke } from '@tauri-apps/api/core'

/** Commands that may legitimately run long (model calls, downloads, restores). */
const LONG: Set<string> = new Set(['ask', 'recap', 'entity_profile', 'restore_backup', 'backup_now', 'set_notion_token', 'export_to_notion', 'meeting_summary', 'stop_meeting', 'send_labels_now'])

/**
 * Every command has a deadline. A command that never answers used to leave
 * a button doing nothing; now it rejects with a message the UI can show.
 */
function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const ms = LONG.has(cmd) ? 10 * 60_000 : 30_000
  let timer: ReturnType<typeof setTimeout> | undefined
  const deadline = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`Lane did not answer "${cmd.replace(/_/g, ' ')}" within ${ms / 1000}s. Please tell us: this should never happen.`)), ms)
  })
  return Promise.race([tauriInvoke<T>(cmd, args), deadline]).finally(() => clearTimeout(timer)) as Promise<T>
}
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface ActivitySummary {
  id: number
  appName: string
  appPath: string | null
  windowTitle: string
  url: string | null
  startedAt: number
  endedAt: number
  snapshotCount: number
  preview: string | null
}

export interface Snapshot {
  id: number
  capturedAt: number
  /** Cleaned text: what search indexes. */
  text: string
  /** The raw accessibility dump it came from. */
  raw: string
  /** Path of the picture kept with it, when screenshots are on. */
  image: string | null
}

export interface ActivityDetail {
  activity: ActivitySummary
  snapshots: Snapshot[]
}

export interface SearchHit {
  activity: ActivitySummary
  snippet: string
}

export interface Licence {
  state: 'trial' | 'licensed' | 'expired'
  daysLeft: number
  plan: string
  email: string
  trialStartedAt: number
  blocked: boolean
}

export interface Settings {
  /** What the person asked to be called; empty falls back to the Mac account. */
  displayName: string
  intervalSecs: number
  idleMinutes: number
  excludedApps: string[]
  excludedUrlPatterns: string[]
  readBrowserText: boolean
  excludeMessaging: boolean
  excludeEmail: boolean
  onboardingDone: boolean
  redactContacts: boolean
  model: string
  trustedBuild: string
  indexFolders: string[]
  indexFiles: boolean
  keepAudio: boolean
  markdownFolder: string
  meetingLanguage: string
  backupEnabled: boolean
  backupFolder: string
  lastBackupAt: number
  calendarEnabled: boolean
  notionEnabled: boolean
  notionParentPage: string
  notionLastSyncAt: number
  rawRetentionDays: number
  meetingNotifications: boolean
  mailEnabled: boolean
  mailMemories: boolean
  contactsEnabled: boolean
  contactsLastSyncAt: number
  clipboardEnabled: boolean
  morningBriefAt: string
  eveningBriefAt: string
  notchEnabled: boolean
  diarizeEnabled: boolean
  screenshotsEnabled: boolean
  autoListenCalls: boolean
  notchPosition: 'top-center' | 'top-left' | 'top-right' | 'bottom-center' | 'left' | 'right'
  notchPositionChosen: boolean
  purposeWhy: string
  purposeHow: string[]
  signalWeights: Record<string, number>
  thoughtCheckEnabled: boolean
  contributeLabels: boolean
  testerToken: string
  labelsLastSentAt: number
}

export interface CalendarEvent {
  id: string
  title: string
  start: number
  end: number
  allDay: boolean
  location: string
  notes: string
  calendar: string
  attendees: string[]
  organizer: string
  /** What to ask Lane to prepare for it. */
  question: string
}

export interface NotionStatus {
  enabled: boolean
  connected: boolean
  workspace: string
  parentPage: string
  pages: number
  lastSyncAt: number
  detail: string
}

export interface LabelsStatus {
  enabled: boolean
  unsent: [number, string, string][]
  lastSentAt: number
  detail: string
  endpoint: string
}

export interface BackupStatus {
  enabled: boolean
  folder: string
  lastBackupAt: number
  latestFile: string | null
  hasPassphrase: boolean
  detail: string
}

export interface Meeting {
  id: number
  activityId: number
  title: string
  startedAt: number
  endedAt: number | null
  status: 'recording' | 'transcribing' | 'done' | 'failed'
  detail: string
  memoryTitle: string | null
  memorySummary: string | null
  /** Notes written after the transcript (Markdown-ish); empty until then. */
  notes: string
  /** Names seen in the meeting app while recording. */
  attendees: string[]
  /** The audio was kept and can be played. */
  hasAudio: boolean
  /** Transcript speaker labels and the names given to them. */
  speakers: Speaker[]
}

export interface NotchContext {
  app: string
  title: string
  url: string | null
  last: { title: string; summary: string; at: number; activityId: number } | null
  person: { id: number; name: string; owed: string[]; lastTitle: string | null; lastAt: number | null } | null
  /** What this window connects to: [name, memories together]. */
  connects: [string, number][]
  notchApps: string[]
}

export interface Signal {
  id: number
  day: string
  kind: 'task' | 'event' | 'date' | 'person' | 'thread' | 'memory'
  refId: number
  activityId: number
  title: string
  reason: string
  score: number
  rank: number
  state: 'open' | 'pinned' | 'done' | 'noise'
  features: Record<string, number>
}

export interface SignalsReport {
  day: string
  signals: Signal[]
  /** [score 0..1, memories, near] for today, when a why is set. */
  alignment: [number, number, number] | null
  whySet: boolean
}

export interface Circle {
  why: string
  how: string[]
  projects: [string, number][]
  dots: { id: number; activityId: number; title: string; kind: string; at: number; alignment: number; project: string | null }[]
  days: [string, number, number, number][]
}

export interface Speaker {
  label: string
  name: string
}

export interface RecordingReport {
  recording: boolean
  meetingId: number | null
  startedAt: number | null
  micOk: boolean
  systemOk: boolean
  detail: string
  liveText: string
  micSeconds: number
  systemSeconds: number
  ready: boolean
  detailReady: string
  dictating: boolean
}

export interface FileHit {
  fileId: number
  path: string
  name: string
  ext: string
  mtime: number
  size: number
  snippet: string
  chunkId: number
  url: string
}

export interface ConnectorReport {
  id: string
  name: string
  kind: 'http' | 'script' | 'mcp' | 'invalid'
  everyMinutes: number
  memories: boolean
  lastRun: number
  items: number
  error: string
  file: string
}

export interface Upcoming {
  when: number
  label: string
  title: string
  activityId: number
  memoryId: number
  about: string
}

export interface Gap {
  kind: 'person' | 'meeting'
  text: string
  entityId: number | null
  question: string
}

export interface Explore {
  /** Minutes in the last 30 days by what the app or site is for. */
  byCategory: [string, number][]
  /** The places you spent most time in: name, category, minutes. */
  topPlaces: [string, string, number][]
  memoriesPerDay: [string, number][]
  minutesPerDay: [string, number][]
  byKind: [string, number][]
  topPeople: Entity[]
  topOrgs: Entity[]
  topProjects: Entity[]
  facts: number
  conflicts: number
  tasksOpen: number
  tasksDone: number
  files: number
  meetings: number
  memories: number
  kept: number
  pinned: number
  edited: number
}

export interface ForgetReport {
  entities: number
  memories: number
  facts: number
  tasks: number
  snapshots: number
  recaps: number
  files: number
}

export interface ScreenSummary {
  appName: string
  windowTitle: string
  url: string | null
  chars: number
}

export interface McpConfig {
  binary: string
  present: boolean
  config: string
  claudeDesktopFile: string
}

export interface FileReport {
  files: number
  chunks: number
  unembedded: number
  lastIndexedAt: number
  /** Extension and how many indexed files carry it, biggest first. */
  byExt: [string, number][]
  folders: string[]
  pending: number
  detail: string
}

export interface FactRef {
  factId: number
  memoryId: number
  value: string
  asOf: number
  title: string
  activityId: number
}

export interface Fact {
  id: number
  memoryId: number
  subject: string
  attribute: string
  value: string
  asOf: number
  origin: 'model' | 'user'
  owner: 'mine' | 'theirs' | 'unknown'
  stance: 'stated' | 'proposed' | 'agreed' | 'asked'
  conflicts: FactRef[]
}

export interface MemoryEdit {
  title: string
  summary: string
  people: string[]
  organizations: string[]
  projects: string[]
  decisions: string[]
}

export interface MemoryCard {
  id: number
  activityId: number
  createdAt: number
  kind: string
  title: string
  summary: string
  people: string[]
  organizations: string[]
  dates: string[]
  numbers: string[]
  projects: string[]
  decisions: string[]
  keep: boolean
  confidence: number
  dropped: number
  model: string
  feedback: 'keep' | 'ignore' | null
  appName: string
  windowTitle: string
  url: string | null
  startedAt: number
  endedAt: number
  groupKey: string
  /** Sessions merged into this card. */
  sessions: number
  totalMs: number
  /** Memory ids in the group; thumbs apply to all. */
  ids: number[]
  pinned: boolean
  editedAt: number | null
  facts: Fact[]
}

export interface AskSource {
  n: number
  card: MemoryCard | null
  file: FileHit | null
  task: Task | null
  entity: Entity | null
}

export interface Entity {
  id: number
  kind: 'person' | 'org' | 'project'
  name: string
  mentions: number
  firstSeen: number
  lastSeen: number
}

export interface Task {
  id: number
  memoryId: number
  text: string
  status: 'open' | 'done' | 'dismissed'
  createdAt: number
  title: string
  appName: string
  startedAt: number
  activityId: number
}

export interface Graph {
  nodes: Entity[]
  edges: { source: number; target: number; weight: number }[]
}

export interface BoardLink {
  id: number
  fromKey: string
  toKey: string
  label: string
  createdAt: number
}

export interface BoardNote {
  key: string
  text: string
  createdAt: number
}

export interface BoardData {
  nodes: Entity[]
  edges: { source: number; target: number; weight: number }[]
  memories: { card: MemoryCard; entityIds: number[] }[]
  positions: [string, number, number][]
  links: BoardLink[]
  notes: BoardNote[]
  placed: MemoryCard[]
}

export interface Conversation {
  id: number
  title: string
  createdAt: number
  updatedAt: number
  messages: number
}

export interface ChatMessage {
  id: number
  role: 'user' | 'assistant'
  content: string
  sources: string
  createdAt: number
}

export interface AskTurn {
  question: string
  answer: string
}

export interface AskResult {
  id: number
  answer: string
  sources: AskSource[]
  error: string | null
  followups: string[]
  unverified: string[]
  checked: boolean
  /** The first answer ignored the question's period or person and was rewritten. */
  scopeFixed?: boolean
}

export interface DayStats {
  memories: number
  openTasks: Task[]
  openTaskCount: number
  timeByApp: [string, number][]
}

export interface EngineReport {
  available: boolean
  model: string
  detail: string
  busy: boolean
  processed: number
  lastError: string | null
  dropped: number
  backend: 'bundled' | 'ollama' | ''
  downloadPercent: number | null
  tokensPerSecond: number
  filesIndexed: number
  filesPending: number
  filesDetail: string
  notionDetail: string
  counts: { pending: number; memories: number; kept: number }
}

export interface PermissionReport {
  accessibilityTrusted: boolean
  /** Trusted and calls succeed. Trusted but not working = restart needed. */
  accessibilityWorks: boolean
  /** Installed .app vs development build started from a terminal. */
  bundled: boolean
  /** The name macOS shows in Privacy & Security → Accessibility. */
  listedAs: string
  executable: string
  launchAtLogin: boolean
  screenRecording: boolean
  /** Granted to an earlier build of the app; macOS needs a fresh approval. */
  staleGrant: boolean
}

export interface PrivacyLists {
  alwaysSkipped: string[]
  messagingApps: string[]
  messagingSites: string[]
  emailApps: string[]
  emailSites: string[]
}

export interface Status {
  paused: boolean
  dbPath: string
  capture: {
    trusted: boolean
    idle: boolean
    excluded: boolean
    currentApp: string | null
    currentTitle: string | null
    lastCaptureAt: number | null
  }
  stats: {
    activities: number
    snapshots: number
    activitiesToday: number
    dbSizeBytes: number
  }
  userInitial: string
  /** The account's full name, when macOS gives us one. */
  userName: string | null
}

/** Snippet highlight markers emitted by the store (see store.rs). */
export const HL_START = ''
export const HL_END = ''

function startOfToday(): number {
  const d = new Date()
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

export const api = {
  status: () => invoke<Status>('get_status', { todayStart: startOfToday() }),
  /** The icon for a memory's source, found on this Mac: site favicon, else app icon. */
  sourceIcon: (app: string, url?: string | null) => invoke<string | null>('source_icon', { app, url: url ?? null }),
  /** True macOS full screen, for the board's walkthrough. */
  setFullscreen: (on: boolean) => invoke<void>('set_fullscreen', { on }),
  /** Record the screen for a fixed run, for saving a board walkthrough. */
  recordScreen: (seconds: number, name: string) => invoke<string>('record_screen', { seconds, name }),
  /** Write down whatever is in front right now, keeping it whatever the filters say. */
  captureNow: () => invoke<string>('capture_now'),
  /** Save a picture the app drew to the Desktop. */
  savePng: (name: string, dataUrl: string) => invoke<string>('save_png', { name, dataUrl }),
  licence: () => invoke<Licence>('licence_status'),
  applyLicence: (key: string) => invoke<Licence>('apply_licence', { key }),
  clearLicence: () => invoke<Licence>('clear_licence'),
  onLicence: (f: () => void) => listen('licence-changed', () => f()),
  setPaused: (paused: boolean) => invoke<void>('set_paused', { paused }),
  permissions: () => invoke<PermissionReport>('get_permissions'),
  requestAccessibility: () => invoke<boolean>('request_accessibility'),
  openSettingsPane: (pane: 'accessibility' | 'loginItems') => invoke<void>('open_settings_pane', { pane }),
  resetAccessibility: () => invoke<void>('reset_accessibility'),
  setLaunchAtLogin: (enabled: boolean) => invoke<boolean>('set_launch_at_login', { enabled }),
  restartApp: () => invoke<void>('restart_app'),
  privacyLists: () => invoke<PrivacyLists>('get_privacy_lists'),
  listActivities: (before?: number, limit = 100) =>
    invoke<ActivitySummary[]>('list_activities', { before: before ?? null, limit }),
  getActivity: (id: number) => invoke<ActivityDetail | null>('get_activity', { id }),
  search: (query: string, limit = 50) => invoke<SearchHit[]>('search', { query, limit }),
  deleteActivity: (id: number) => invoke<boolean>('delete_activity', { id }),
  getSettings: () => invoke<Settings>('get_settings'),
  updateSettings: (settings: Settings) => invoke<Settings>('update_settings', { settings }),
  wipeAll: () => invoke<void>('wipe_all'),
  listMemories: (query?: string, keptOnly = true, limit = 200, droppedOnly = false) =>
    invoke<MemoryCard[]>('list_memories', { query: query ?? null, keptOnly, limit, droppedOnly }),
  memoryFeedback: (ids: number[], feedback: 'keep' | 'ignore' | null) => invoke<void>('memory_feedback', { ids, feedback }),
  setPinned: (ids: number[], pinned: boolean) => invoke<void>('set_pinned', { ids, pinned }),
  updateMemory: (id: number, edit: MemoryEdit) => invoke<MemoryCard>('update_memory', { id, edit }),
  memoryFacts: (ids: number[]) => invoke<Fact[]>('memory_facts', { ids }),
  addFact: (memoryId: number, subject: string, attribute: string, value: string) => invoke<number>('add_fact', { memoryId, subject, attribute, value }),
  retractFact: (id: number) => invoke<void>('retract_fact', { id }),
  correctFact: (id: number, value: string) => invoke<number>('correct_fact', { id, value }),
  conflictingFacts: (limit = 50) => invoke<Fact[]>('conflicting_facts', { limit }),
  engineStatus: () => invoke<EngineReport>('engine_status'),
  processNow: () => invoke<void>('process_now'),
  exportLabels: () => invoke<string>('export_labels'),
  ask: (question: string, history: AskTurn[] = [], mode: 'answer' | 'draft' = 'answer', conversationId?: number | null, persist = true) =>
    invoke<{ id: number; conversationId: number | null }>('ask', { question, history, mode, conversationId: conversationId ?? null, persist }),
  insights: () => invoke<string[]>('insights'),
  notchShow: () => invoke<void>('notch_show'),
  notchHide: () => invoke<void>('notch_hide'),
  notchResize: (width: number, height: number) => invoke<void>('notch_resize', { width, height }),
  dictationToggle: () => invoke<string>('dictation_toggle'),
  onDictationState: (cb: (p: { active: boolean }) => void): Promise<UnlistenFn> => listen<{ active: boolean }>('dictation-state', (e) => cb(e.payload)),
  onAskFollowups: (cb: (p: { id: number; followups: string[] }) => void): Promise<UnlistenFn> =>
    listen<{ id: number; followups: string[] }>('ask-followups', (e) => cb(e.payload)),
  listConversations: (limit = 50) => invoke<Conversation[]>('list_conversations', { limit }),
  conversationMessages: (id: number) => invoke<ChatMessage[]>('conversation_messages', { id }),
  renameConversation: (id: number, title: string) => invoke<void>('rename_conversation', { id, title }),
  deleteConversation: (id: number) => invoke<void>('delete_conversation', { id }),
  recap: (ms: number, force = false, span: 'day' | 'week' | 'month' = 'day') => invoke<number>('recap', { ms, force, span }),
  upcomingDates: (days = 30) => invoke<Upcoming[]>('upcoming_dates', { days }),
  memoryGaps: () => invoke<Gap[]>('memory_gaps'),
  explore: () => invoke<Explore>('explore'),
  setAlias: (entityId: number, alias: string) => invoke<string[]>('set_alias', { entityId, alias }),
  removeAlias: (entityId: number, alias: string) => invoke<string[]>('remove_alias', { entityId, alias }),
  quitApp: () => invoke<void>('quit_app'),
  diagnosticsBundle: () => invoke<string>('diagnostics_bundle'),
  screenContext: () => invoke<ScreenSummary | null>('screen_context'),
  askScreen: (question: string, history: AskTurn[] = []) => invoke<number>('ask_screen', { question, history }),
  forgetTerm: (term: string) => invoke<ForgetReport>('forget_term', { term }),
  aliasesOf: (entityId: number) => invoke<string[]>('aliases_of', { entityId }),
  addNote: (text: string) => invoke<number>('add_note', { text }),
  listDecisions: (since?: number) => invoke<{ text: string; title: string; activityId: number; at: number }[]>('list_decisions', { since: since ?? null }),
  dayStats: (ms: number) => invoke<DayStats>('day_stats', { ms }),
  hideOverlay: () => invoke<void>('hide_overlay'),
  startMeeting: (title?: string, voiceNote = false) => invoke<number>('start_meeting', { title: title ?? null, voiceNote }),
  stopMeeting: () => invoke<void>('stop_meeting'),
  recordingStatus: () => invoke<RecordingReport>('recording_status'),
  listMeetings: (limit = 100) => invoke<Meeting[]>('list_meetings', { limit }),
  renameMeeting: (id: number, title: string) => invoke<void>('rename_meeting', { id, title }),
  labelsStatus: () => invoke<LabelsStatus>('labels_status'),
  excludeLabel: (id: number) => invoke<void>('exclude_label', { id }),
  sendLabelsNow: () => invoke<number>('send_labels_now'),
  backupStatus: () => invoke<BackupStatus>('backup_status'),
  setBackupPassphrase: (passphrase: string) => invoke<string>('set_backup_passphrase', { passphrase }),
  backupNow: () => invoke<string>('backup_now'),
  disableBackups: () => invoke<void>('disable_backups'),
  chooseBackupFile: () => invoke<string | null>('choose_backup_file'),
  restoreBackup: (path: string, passphrase: string) => invoke<void>('restore_backup', { path, passphrase }),
  meetingSummary: (id: number, transcript = false) => invoke<string>('meeting_summary', { id, transcript }),
  meetingNotes: (id: number) => invoke<void>('meeting_notes', { id }),
  openMeetingAudio: (id: number, reveal = false) => invoke<void>('open_meeting_audio', { id, reveal }),
  deleteMeeting: (id: number) => invoke<void>('delete_meeting', { id }),
  listFiles: (limit = 40) => invoke<FileHit[]>('list_files', { limit }),
  renameSpeaker: (id: number, label: string, name: string) => invoke<number>('rename_speaker', { id, label, name }),
  snapshotImage: (path: string) => invoke<string>('snapshot_image', { path }),
  requestScreenRecording: () => invoke<boolean>('request_screen_recording'),
  notchContext: () => invoke<NotchContext>('notch_context'),
  signals: (ms?: number) => invoke<SignalsReport>('signals', { ms: ms ?? null }),
  setSignalState: (id: number, newState: Signal['state']) => invoke<void>('set_signal_state', { id, newState }),
  rankSignals: (ids: number[]) => invoke<void>('rank_signals', { ids }),
  refreshSignals: () => invoke<number>('refresh_signals'),
  circle: () => invoke<Circle>('circle'),
  onSignalsChanged: (cb: () => void): Promise<UnlistenFn> => listen('signals-changed', cb),
  onNotchPosition: (cb: (p: string) => void): Promise<UnlistenFn> => listen<string>('notch-position', (e) => cb(e.payload)),
  speakerToolkit: (fetch = false) => invoke<{ ready: boolean; detail: string }>('speaker_toolkit', { fetch }),
  copyText: (text: string) => invoke<void>('copy_text', { text }),
  saveMarkdown: (name: string, text: string) => invoke<string>('save_markdown', { name, text }),
  onMeetingsChanged: (cb: () => void): Promise<UnlistenFn> => listen('meetings-changed', cb),
  searchFiles: (query: string, limit = 20) => invoke<FileHit[]>('search_files', { query, limit }),
  fileStats: () => invoke<FileReport>('file_stats'),
  revealFile: (path: string) => invoke<void>('reveal_file', { path }),
  openFile: (path: string) => invoke<void>('open_file', { path }),
  reindexFiles: () => invoke<void>('reindex_files'),
  listEntities: (kind?: string, query?: string, limit = 100) =>
    invoke<Entity[]>('list_entities', { kind: kind ?? null, query: query ?? null, limit }),
  entityMemories: (id: number, limit = 50) => invoke<MemoryCard[]>('entity_memories', { id, limit }),
  graph: (maxNodes = 150, minMentions = 1) => invoke<Graph>('graph', { maxNodes, minMentions }),
  board: (since?: number, maxNodes = 200, minMentions = 1, maxMemories = 120) =>
    invoke<BoardData>('board', { since: since ?? null, maxNodes, minMentions, maxMemories }),
  memoryDetail: (id: number) => invoke<{ card: MemoryCard; text: string }>('memory_detail', { id }),
  addBoardLink: (fromKey: string, toKey: string, label = '') => invoke<number>('add_board_link', { fromKey, toKey, label }),
  setBoardLinkLabel: (id: number, label: string) => invoke<void>('set_board_link_label', { id, label }),
  removeBoardLink: (id: number) => invoke<void>('remove_board_link', { id }),
  setBoardNote: (key: string, text: string, x: number, y: number) => invoke<void>('set_board_note', { key, text, x, y }),
  removeBoardNode: (key: string) => invoke<void>('remove_board_node', { key }),
  setBoardPosition: (key: string, x?: number, y?: number) => invoke<void>('set_board_position', { key, x: x ?? null, y: y ?? null }),
  listTasks: (status: 'open' | 'done' | 'dismissed' = 'open', limit = 200) => invoke<Task[]>('list_tasks', { status, limit }),
  setTaskStatus: (id: number, status: 'open' | 'done' | 'dismissed') => invoke<void>('set_task_status', { id, status }),
  showOverlay: () => invoke<void>('show_overlay'),
  openMain: (page?: string, activity?: number) => invoke<void>('open_main', { page: page ?? null, activity: activity ?? null }),
  onOverlayShown: (cb: () => void): Promise<UnlistenFn> => listen('overlay-shown', cb),
  onNavigate: (cb: (p: { page: string | null; activity: number | null }) => void): Promise<UnlistenFn> =>
    listen<{ page: string | null; activity: number | null }>('navigate', (e) => cb(e.payload)),
  onAskToken: (cb: (p: { id: number; token: string }) => void): Promise<UnlistenFn> =>
    listen<{ id: number; token: string }>('ask-token', (e) => cb(e.payload)),
  onAskDone: (cb: (r: AskResult) => void): Promise<UnlistenFn> => listen<AskResult>('ask-done', (e) => cb(e.payload)),
  upcomingEvents: (hours = 24) => invoke<CalendarEvent[]>('upcoming_events', { hours }),
  notionStatus: () => invoke<NotionStatus>('notion_status'),
  setNotionToken: (token: string) => invoke<string>('set_notion_token', { token }),
  disconnectNotion: () => invoke<void>('disconnect_notion'),
  syncNotionNow: () => invoke<void>('sync_notion_now'),
  exportToNotion: (title: string, text: string) => invoke<string>('export_to_notion', { title, text }),
  listConnectors: () => invoke<ConnectorReport[]>('list_connectors'),
  runConnector: (id: string) => invoke<void>('run_connector', { id }),
  openConnectorsFolder: () => invoke<string>('open_connectors_folder'),
  setConnectorSecret: (id: string, secret: string) => invoke<void>('set_connector_secret', { id, secret }),
  mcpConfig: () => invoke<McpConfig>('mcp_config'),
  entityProfile: (id: number, force = false) => invoke<number>('entity_profile', { id, force }),
  onMemoriesChanged: (cb: () => void): Promise<UnlistenFn> => listen('memories-changed', cb),
  onActivityChanged: (cb: () => void): Promise<UnlistenFn> => listen('activity-changed', cb),
}

export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000))
  if (s < 60) return `${s}s`
  const m = Math.round(s / 60)
  if (m < 60) return `${m} min`
  const h = Math.floor(m / 60)
  return `${h}h ${m % 60}m`
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / (1024 * 1024)).toFixed(1)} MB`
}

export function hostOf(url: string | null): string | null {
  if (!url) return null
  try {
    const u = new URL(url)
    return u.protocol === 'file:' ? decodeURIComponent(u.pathname.split('/').pop() || '') : u.host
  } catch {
    return null
  }
}
