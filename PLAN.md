
# kagent / sango-aware Kitty Agent Dashboard 企画書

## 1. 概要

本企画は、kitty terminal 上で複数の AI agent を使って開発する際に、各 agent tab/window の状態、会話内容、入力待ち、承認待ち、実行中タスク、関連 repo、diff、service health、port、log を親フォルダ単位で統合表示する Rust 製 TUI を作るものである。

仮称は `kagent` とする。sango と密接に連携するが、sango 本体の TUI 置き換えではない。sango は multi-repo / worktree / service / port / log / health の正本として扱い、kagent は kitty / AI agent session / TUI / notification / navigation を担当する。

中心画面は **Agent Lens** とする。Worktree Matrix や Service Lens も重要だが、最初に作るべきホーム画面は、cmux / Orca 的な「複数 agent の状況一覧」に、sango の multi-repo 文脈を結合したものである。

一文で定義すると次の通り。

> kitty 上の各 AI agent tab/window を観測可能な session として扱い、会話プレビューと needs input / needs approval を中心に、sango の multi-repo worktree / service / port / log / health 情報を結合して表示する Rust 製 TUI。

## 2. 背景と課題

現在の cmux や Orca に近い体験は、単一 workspace / 単一 repo / agent 単位の可視化には有効だが、個人開発でよくある親フォルダ配下の multi-repo 構成には十分対応できていない。

想定する開発構成は次のようなもの。

```text
~/dev/my-product/
  sango.yaml
  web/
  bff/
  backend/
```

この親フォルダは、チームで管理された monorepo ではなく、個人の作業 anchor である。親フォルダ自体が Git repo であってもよいが、必須ではない。主な作業は、親フォルダで `codex` や `claude` を起動し、複数 repo を横断して開発する形になる。

この運用では、次の情報を一つの画面で見たい。

- kitty の各 tab/window で動いている AI agent の名前と状態
- 各 agent との実際のやり取りのプレビュー
- needs input / needs approval / failed / running / done などの状態
- どの agent がどの repo に影響しているか
- web / bff / backend など各 repo の diff 数、dirty 状態、branch
- sango で起動した service、port、health、log、依存関係
- 問題のある agent / service / repo への遷移

## 3. 基本方針

### 3.1 ホーム画面は Agent Lens

最初に作るべき画面は Worktree Matrix ではなく Agent Lens である。

理由は、ユーザーが最も見たいものが「各 agent tab で何が起きているか」だからである。multi-repo 状態や service 状態は重要だが、Agent Lens の context として結合する。

Agent Lens の主目的は次の通り。

```text
複数 agent tab の状態、会話、入力待ち、影響範囲を一覧・監視し、
必要な tab / repo / service / diff / log へ即座に遷移する。
```

### 3.2 sango は runtime / worktree provider

sango は置き換えない。sango は次の情報の正本である。

- project root
- services
- profiles
- ports
- healthcheck
- logs
- worktree set
- repo/worktree 状態
- doctor / troubleshoot / runbook

kagent は sango の JSON interface を読み取り、kitty / agent session / git scan と結合して表示する。

### 3.3 Rust TUI として作る

実装言語は Rust とする。Yazi を参考にできること、TUI の描画・状態管理・非同期 preview・pane 操作を作り込みたいことが理由である。

最初から単なる CLI 出力にしない。TUI ライブラリとしての再利用性を意識し、core / ui / provider / lens / widget を分離する。

### 3.4 文字列出力で画面を作らない

これは強い制約とする。

禁止する設計:

```text
- println! と手作業の padding で表を作る
- 文字列連結で pane や margin を調整する
- terminal 幅に対して ad-hoc な if 文でレイアウトする
- UI component と business logic を同じ関数に混ぜる
- 見た目を後回しにして、単なる status text dump にする
```

採用する設計:

```text
- Ratatui 等の modern TUI framework を使う
- Widget / StatefulWidget / layout constraint を使って描画する
- pane、list、preview、table、status bar、help、modal を component 化する
- layout は declarative な constraint と pane state で管理する
- Yazi / lazygit を参考に、graphical terminal UI としての操作感を追求する
```

このプロダクトは「文字情報を terminal に並べるツール」ではなく、「kitty-native な agent observability dashboard」である。

## 4. 非目標

v0 では以下をやらない。

- 既存 kitty tab の完全な live terminal embedding
- dashboard から agent に自由入力する機能
- browser 操作
- PR review UI
- commit / reset / clean などの破壊的 Git 操作
- 常駐 daemon 必須設計
- 巨大な plugin system
- AI 出力の高度な自然言語解析に依存する状態判定

Enter で実際の kitty tab/window へ focus することを基本にする。approval や通常入力は、v0 では実 terminal 側で行う。

## 5. コア概念

### 5.1 ProjectRoot

親フォルダを `ProjectRoot` と呼ぶ。`sango.yaml` がある場所を優先的に ProjectRoot とする。

```text
ProjectRoot
├── repos
├── worktree_sets
├── services
├── agents
├── kitty_targets
└── local_state
```

親フォルダが Git repo である必要はない。個人用 manifest repo として使ってもよいが、dashboard の主キーにはしない。

### 5.2 AgentSlot

UI 上は「tab ごとの agent」として見せるが、内部では kitty の tab/window を区別する。

```text
AgentSlot
= AI agent session と紐づいた observable terminal target
```

kitty では tab の中に split window があるため、実装上は `kitty_window_id` を主キーにする。

### 5.3 Tracked と Inferred

agent session は二種類ある。

```text
tracked:
  kagent run 経由で起動された session。
  session id、baseline、terminal capture、events、kitty target が記録される。

inferred:
  直接 codex / claude を起動した session。
  kitty cwd、cmdline、screen text から推定する。
```

UI では明確に区別する。

```text
● codex  auth-refactor   tracked
? claude main            inferred
```

Tracked session は session diff や会話プレビューが比較的正確になる。Inferred session は fallback として表示するが、needs input 判定や会話抽出には `?` を付ける。

### 5.4 AgentStatus と Attention

`status` と `attention` は分ける。

```rust
enum AgentStatusKind {
    Starting,
    Streaming,
    ToolRunning,
    Running,
    Idle,
    NeedsInput,
    NeedsApproval,
    Blocked,
    Done,
    Failed,
    Exited,
    Unknown,
}
```

```rust
enum AttentionLevel {
    None,
    Info,
    NeedsUser,
    Error,
}
```

例:

```text
status:    NEEDS_APPROVAL
attention: unread needs-user
```

ユーザーが該当 tab を見た後は、status は `NEEDS_APPROVAL` のままでも unread は消せる。

## 6. Agent Lens 仕様

### 6.1 画面の役割

Agent Lens はこのプロダクトのホーム画面である。

見るべきもの:

- agent tab/window 一覧
- agent 名、session 名、status、duration
- 実際のやり取りの preview
- needs input / needs approval の強調
- touched repos
- dirty count / session diff count
- related services
- service health / port
- 選択中 agent の詳細会話
- selected agent に関係する diff / logs / services への導線

### 6.2 基本レイアウト

横幅が十分ある場合は 3 ペイン構成にする。

```text
┌────────────────────────────────────┬──────────────────────────────────────────────┬──────────────────────────────┐
│ Agents / Tabs                      │ Conversation Preview                         │ Context / Impact              │
├────────────────────────────────────┼──────────────────────────────────────────────┼──────────────────────────────┤
│ Filter: all   Needs: 3   Active: 8 │ codex / auth-refactor                         │ Scope                         │
│                                    │ Status: NEEDS_APPROVAL                        │   WorktreeSet: auth-refactor  │
│ ! 01 codex  auth-refactor  00:48   │ Source: tracked / adapter                     │   CWD: ~/dev/my-product       │
│   "Approve command: pnpm test?"    │                                              │                              │
│   web Δ8 bff Δ2 | web✓ bff✓ api!   │ [12:31:04] user                               │ Repos                         │
│                                    │ > fix login flow across web and bff            │   web      feat/auth   Δ8     │
│ ● 02 claude checkout-flow running  │                                              │   bff      feat/auth   Δ2     │
│   "I found the failing route..."   │ [12:32:10] codex                              │   backend  main        clean  │
│   backend Δ5 | api✓ db✓            │ I changed the BFF session route and updated    │                              │
│                                    │ the frontend login API client.                 │ Services                      │
│ ? 03 codex docs-cleanup inferred   │                                              │   web      OK    :3101        │
│   "No visible prompt"              │ [12:33:02] tool                               │   bff      OK    :4101        │
│   docs Δ3                          │ $ pnpm test                                   │   backend  FAIL  :5101        │
│                                    │                                              │                              │
│ ✓ 04 claude main done              │ [12:33:40] needs approval                     │ Attention                     │
│   "Summary written"                │ Allow command? pnpm test -- --watch           │   approval waiting 00:48      │
│                                    │                                              │                              │
│                                    │                                              │ Actions                       │
│                                    │                                              │   Enter focus tab             │
│                                    │                                              │   d diff   l logs   s service │
└────────────────────────────────────┴──────────────────────────────────────────────┴──────────────────────────────┘
```

この見た目はあくまで構造説明である。実装時には文字列の手作業ではなく、Widget と layout engine によって描画する。

### 6.3 Pane 状態

lazygit のように、あまり見ない pane は縮小表示し、必要な時に拡大できるようにする。

```rust
enum PaneSize {
    Hidden,
    Compact,
    Normal,
    Expanded,
    Maximized,
}
```

Agent Lens の初期状態:

```text
Agents:       Normal
Conversation: Expanded
Context:      Compact or Normal
```

画面幅ごとの layout:

```text
>= 160 cols:
  Agents + Conversation + Context

120-159 cols:
  Agents + Conversation
  Context は bottom compact strip

< 120 cols:
  focus pane を切替表示
```

操作:

```text
z       current pane maximize / restore
[       context pane shrink
]       context pane expand
Tab     preview mode switch
```

### 6.4 Agent row

agent 一覧は 1 session を 2 行で表示する。

```text
! 01 codex  auth-refactor   NEEDS_APPROVAL  00:48
  "Approve command: pnpm test -- --watch?"  web Δ8 bff Δ2 | web✓ bff✓ backend!
```

別例:

```text
● 02 claude checkout-flow   STREAMING        00:03
  "I found the issue in routes/checkout.ts"  backend Δ5 | api✓ db✓
```

```text
? 03 codex  docs-cleanup    NEEDS_INPUT?     05:12
  "Continue? [y/N]"                         docs Δ3 | inferred
```

```text
✓ 04 claude main            DONE             12:44
  "Summary written to docs/release.md"       clean
```

Row に含める情報:

- status icon
- kitty tab/window index
- agent kind
- session name
- status label
- status duration
- last useful message
- touched repos
- service health summary
- tracked / inferred

Status icon:

```text
!   unread needs input / approval
▲   read but still needs input / approval
●   running / streaming
○   idle
✓   done
×   failed
?   inferred / uncertain
```

### 6.5 Conversation Preview

中央ペインは、選択中 agent の会話・terminal 状態を表示する。

Preview mode:

```text
1. Transcript
2. Raw Terminal
3. Last Screen
4. Events
5. Summary
```

#### Transcript mode

Tracked session 用の本命表示。

```text
[12:31:04] user
> fix login flow across web and bff

[12:32:10] codex
I changed the BFF session route and updated the frontend API client.

[12:33:02] tool
$ pnpm test

[12:33:40] needs approval
Allow command? pnpm test -- --watch
```

#### Raw Terminal mode

PTY capture の raw output を表示する。parse に失敗した場合の escape hatch。

#### Last Screen mode

kitty の screen text を表示する。Inferred session では基本的にこの mode が使われる。

#### Events mode

status transition と通知を時系列表示する。

```text
12:31:04 session_started
12:32:10 tool_started pnpm test
12:33:40 needs_approval
12:34:28 user_focused
```

#### Summary mode

構造化情報の要約。

```text
Session: codex/auth-refactor
Status: Needs approval
Touched repos:
  web Δ8
  bff Δ2
Services:
  web OK
  bff OK
  backend FAIL
Last message:
  Approve command: pnpm test -- --watch
```

### 6.6 Context / Impact

右ペインは、選択中 agent が multi-repo / service 環境に与えている影響を見るための pane である。

表示項目:

- scope
- cwd
- worktree set
- touched repos
- branch
- dirty files
- session diff count
- related services
- service health
- ports
- failing services
- related recent logs
- related files
- actions

Compact 表示:

```text
Impact: auth-refactor | web Δ8 bff Δ2 backend clean | web✓ bff✓ backend! | approval 00:48
```

Expanded 表示:

```text
Repos
  web      feature/auth-refactor   Δ8   session Δ5
  bff      feature/auth-refactor   Δ2   session Δ2
  backend  main                    clean

Services
  web      OK    :3101   pid 18302
  bff      OK    :4101   pid 18344
  backend  FAIL  :5101   healthcheck /health failed

Related logs
  backend 12:14:04 migration missing

Related files
  web/src/api/auth.ts
  bff/routes/auth.ts
```

### 6.7 Filter / Sort

Filter は必須。

```text
all
needs-input
running
failed
tracked
inferred
worktree:<name>
repo:<name>
service:<name>
agent:codex
agent:claude
```

`/` で filter 入力。

```text
/ needs
/ repo:web
/ auth-refactor
/ claude
```

Sort mode:

```text
attention
kitty-order
updated-at
worktree-set
agent-kind
session-name
```

Default は `attention`。

優先順:

```text
1. unread needs input / approval
2. failed
3. read needs input / approval
4. streaming / running
5. idle
6. done
```

### 6.8 Keymap

```text
j/k       move selection
h/l       pane focus / drill in
Enter     focus selected kitty tab/window
Tab       preview mode switch
1         Agent Lens
2         Worktree Matrix Lens
3         Repo Lens
4         Service Lens
5         Diff Lens
6         Logs Lens
7         Doctor Lens
/         filter
S         sort mode switch
n         next attention
m         mark read
r         refresh
z         maximize / restore current pane
[         shrink context pane
]         expand context pane
d         diff for selected agent
l         logs for selected agent / service
s         services related to selected agent
t         troubleshoot selected service
?         help
q         quit
```

v0 では dashboard から agent へ自由入力しない。Enter で実 terminal に遷移する。

## 7. Needs Input / Needs Approval 監視

### 7.1 状態判定の source

状態判定は source と confidence を持つ。

```rust
enum StatusSource {
    ExplicitEvent,
    Adapter,
    TerminalHeuristic,
    ProcessState,
    Manual,
}

struct AgentStatus {
    kind: AgentStatusKind,
    source: StatusSource,
    confidence: f32,
    since: DateTime<Utc>,
    message: Option<String>,
}
```

表示ルール:

```text
confidence >= 0.8:
  NEEDS_INPUT

confidence < 0.8:
  NEEDS_INPUT?
```

### 7.2 検出階層

#### Tier 1: Explicit Event

最も信頼できる。agent hook / wrapper から投入する。

```bash
kagent status --session <id> --state needs_approval --message "Approve pnpm test?"
```

#### Tier 2: PTY Capture + Adapter

`kagent run` が child process を PTY 経由で起動し、terminal stream を adapter が分類する。

検出対象例:

```text
Do you want to continue?
Approve?
Allow command?
Proceed? [y/N]
Waiting for input
Press enter to continue
```

Adapter interface:

```rust
trait AgentAdapter {
    fn detect_kind(&self, cmdline: &[String]) -> bool;
    fn classify_frame(&self, frame: &TerminalFrame) -> Vec<AgentEvent>;
    fn extract_preview(&self, frame: &TerminalFrame) -> ConversationPreview;
}
```

Codex / Claude / OpenCode / generic shell 用 adapter を用意する。

#### Tier 3: Kitty Screen Heuristic

直接起動された inferred session 用。kitty から取得した表示テキストを軽く解析する。判定は不確実なので `?` を付ける。

#### Tier 4: Manual Override

ユーザーが手動で状態や unread を調整できる。

```text
m  mark read
i  mark needs input
e  mark error
```

## 8. sango への機能要望

kagent 側で重要なのは、sango に UI を増やすことではなく、machine-readable interface を安定させることである。

### 8.1 P0: `sango snapshot --json`

最重要。dashboard が毎回 `status --json`、`worktree status --json`、`logs --json` を個別に join しなくてもよいように、project 全体の read-only snapshot を返す。

```bash
sango snapshot --json --root ~/dev/my-product
```

必要な情報:

```json
{
  "schema_version": 1,
  "project": {
    "name": "my-product",
    "root": "/Users/me/dev/my-product",
    "active_worktree_set": "auth-refactor"
  },
  "repos": [
    {
      "id": "web",
      "path": "/Users/me/dev/my-product/web",
      "default_branch": "main",
      "services": ["web"]
    }
  ],
  "worktree_sets": [
    {
      "id": "auth-refactor",
      "active": true,
      "repo_worktrees": [
        {
          "repo_id": "web",
          "path": "/Users/me/dev/my-product/.sango/work/auth-refactor/web",
          "branch": "feature/auth-refactor",
          "head": "abc123",
          "dirty_files": 8,
          "status": "dirty"
        }
      ]
    }
  ],
  "services": [
    {
      "id": "web",
      "repo_id": "web",
      "worktree_set": "auth-refactor",
      "type": "process",
      "status": "running",
      "health": "ok",
      "port": 3101,
      "url": "http://localhost:3101",
      "pid": 18302,
      "profile": "backend",
      "depends_on": ["bff"]
    }
  ]
}
```

重要なのは stable id。

```text
repo_id
service_id
worktree_set_id
```

path ではなく ID で join できるようにする。

### 8.2 P0: read command に `--root` を付ける

dashboard は必ずしも project root で起動されるとは限らない。

```bash
sango status --json --root <dir>
sango worktree status --json --root <dir>
sango logs --json --root <dir>
sango doctor --json --root <dir>
sango snapshot --json --root <dir>
```

### 8.3 P0: `sango logs --json` の tail / since / cursor

Agent Lens の context や Logs Lens で軽く log を出したい。

```bash
sango logs backend --json --tail 100
sango logs backend --json --since 2026-05-06T12:00:00Z
sango logs backend --json --cursor <cursor>
```

JSONL 各行:

```json
{
  "ts": "2026-05-06T12:33:21Z",
  "service_id": "backend",
  "level": "error",
  "message": "migration missing",
  "cursor": "backend:18401:00001234"
}
```

### 8.4 P0: worktree matrix 用 JSON

`worktree status --json` は matrix 表示に使う。

```json
{
  "schema_version": 1,
  "active_worktree_set": "auth-refactor",
  "repos": ["web", "bff", "backend"],
  "worktree_sets": [
    {
      "id": "auth-refactor",
      "repo_worktrees": {
        "web": {
          "path": "...",
          "branch": "feature/auth-refactor",
          "head": "abc123",
          "dirty_files": 8,
          "exists": true
        },
        "bff": {
          "path": "...",
          "branch": "feature/auth-refactor",
          "head": "def456",
          "dirty_files": 2,
          "exists": true
        },
        "backend": {
          "path": "...",
          "branch": "main",
          "head": "789abc",
          "dirty_files": 0,
          "exists": true
        }
      }
    }
  ]
}
```

### 8.5 P1: `sango watch --jsonl`

常駐 daemon ではなく、foreground の event stream が欲しい。

```bash
sango watch --jsonl --root ~/dev/my-product
```

出力例:

```json
{"seq":1,"ts":"...","kind":"service_health_changed","service_id":"backend","old":"ok","new":"fail"}
{"seq":2,"ts":"...","kind":"service_started","service_id":"web","pid":18302,"port":3101}
{"seq":3,"ts":"...","kind":"worktree_switched","from":"main","to":"auth-refactor"}
```

これがあれば dashboard は polling 依存を減らせる。

### 8.6 P1: optional `repos` section in `sango.yaml`

現状は `services.*.working_dir` から repo を推定できるが、service を持たない repo や 1 repo 複数 service のために optional な `repos` section が欲しい。

```yaml
repos:
  web:
    path: ./web
    default_branch: main
    tags: [frontend]
    services: [web]

  bff:
    path: ./bff
    default_branch: main
    tags: [gateway]
    services: [bff]

  backend:
    path: ./backend
    default_branch: main
    tags: [api]
    services: [backend]
```

v0 では必須にしない。

### 8.7 P1: `sango context --json|markdown`

agent に渡す context を作るための command。

```bash
sango context --json
sango context --markdown
```

Markdown 例:

```markdown
# Project: my-product

Active worktree set: auth-refactor

## Repos
- web: feature/auth-refactor, dirty 8 files
- bff: feature/auth-refactor, dirty 2 files
- backend: main, clean

## Services
- web: OK, http://localhost:3101
- bff: OK, http://localhost:4101
- backend: FAIL, http://localhost:5101
```

### 8.8 P2: `sango service inspect --json`

Service detail 用。

```bash
sango service inspect backend --json
```

```json
{
  "id": "backend",
  "repo_id": "backend",
  "worktree_set": "auth-refactor",
  "command": "npm start",
  "working_dir": "./backend",
  "port": 5101,
  "healthcheck": {
    "url": "http://localhost:5101/health",
    "status": "fail",
    "last_error": "connection refused"
  },
  "depends_on": ["postgres"]
}
```

### 8.9 sango に入れないもの

以下は kagent 側で担当する。

```text
- kitty tab/window 管理
- agent session tracking
- terminal capture
- needs input classifier
- TUI pane layout
- notification read/unread 管理
- session diff baseline
- UI theme / keymap / modal / focus state
```

## 9. Rust アーキテクチャ

### 9.1 crate 構成

```text
crates/
  kagent-core/
    snapshot model
    agent session model
    event model
    lens model
    status classifier interfaces

  kagent-ui/
    ratatui widgets
    layout engine
    pane state
    keymap
    theme tokens
    command palette

  kagent-kitty/
    kitty remote-control adapter
    list windows
    get text
    focus window
    set title
    launch shell/log pane

  kagent-sango/
    sango command adapter
    snapshot parser
    service/log/worktree model mapping

  kagent-git/
    repo scanner
    diff summary
    session baseline diff

  kagent-agent/
    kagent run wrapper
    PTY capture
    codex adapter
    claude adapter
    generic adapter

  kagent-cli/
    dash
    run
    status
    notify
    context
```

### 9.2 Data flow

```text
KittyPoller ─┐
SangoPoller ─┤
GitPoller ───┤
AgentWatcher ├── EventBus ── SnapshotStore ── Lens Engine ── UI
LogTailer ───┤
Classifier ──┘
```

### 9.3 Provider

```rust
trait Provider {
    fn name(&self) -> &'static str;
    async fn snapshot(&self, ctx: &Context, root: &Path) -> anyhow::Result<PartialSnapshot>;
}
```

Providers:

```text
SangoProvider
KittyProvider
GitProvider
AgentProvider
RuntimeProvider
```

### 9.4 Lens

```rust
trait Lens {
    fn id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn rows(&self, snapshot: &Snapshot, state: &LensState) -> Vec<Row>;
    fn preview(&self, selection: &Selection, snapshot: &Snapshot) -> Preview;
    fn actions(&self, selection: &Selection, snapshot: &Snapshot) -> Vec<Action>;
}
```

Lenses:

```text
Agent Lens
Worktree Matrix Lens
Repo Lens
Service Lens
Diff Lens
Logs Lens
Doctor Lens
Kitty Lens
```

v0 は Agent Lens を最優先にする。

### 9.5 UI component 方針

必須 widgets:

```text
AgentListWidget
ConversationPreviewWidget
ContextImpactWidget
StatusBarWidget
KeyHelpWidget
FilterInputWidget
CommandPaletteWidget
PaneFrameWidget
ScrollableListWidget
DiffPreviewWidget
LogPreviewWidget
ServiceHealthWidget
```

描画方針:

```text
- 各 widget は Rect を受け取り Buffer に描く
- layout 計算は UI root / layout engine に閉じ込める
- data fetching は widget 内で行わない
- widget は Snapshot と ViewState だけを見る
- color / icon / spacing は theme token を経由する
- 文字列 padding で見た目を成立させない
```

### 9.6 Refresh interval

```text
selected agent preview:
  300-700ms

kitty window list:
  1-2s

sango snapshot:
  2-5s
  watch があれば event-driven

git dirty summary:
  active worktree set: 1-3s
  inactive worktree sets: 10-30s

logs:
  selected service only tail
```

全 repo / 全 diff を常時舐めない。選択中 agent に関係する repo / service を優先更新する。

## 10. Local state

親フォルダ配下に保存する。

```text
.sango/
  agents/
    sessions/
      <session_id>/
        metadata.json
        events.jsonl
        terminal.ansi
        transcript.jsonl
        baseline.json
    index.jsonl

  dashboard/
    state.sqlite
    cache/
```

会話内容には secret が含まれる可能性があるため、capture 設定を必須にする。

```toml
[capture]
mode = "transcript" # off | screen | ansi | transcript
redact_secrets = true
pause_key = "ctrl+shift+p"
```

最低限必要な配慮:

```text
- capture off を選べる
- raw ANSI capture と transcript capture を分ける
- secret redaction hook を用意する
- .sango/agents は commit 対象にしない
```

## 11. コマンド仕様

### 11.1 Dashboard

```bash
kagent dash
```

ProjectRoot を検出し、Agent Lens を開く。

### 11.2 Agent wrapper

```bash
kagent run --name auth-refactor --agent codex -- codex
kagent run --name ui-polish --agent claude -- claude
```

日常運用向け alias:

```bash
alias codex='kagent run --agent codex --'
alias claude='kagent run --agent claude --'
```

Wrapper の処理:

```text
1. session_id を発行
2. ProjectRoot / sango root を検出
3. KITTY_WINDOW_ID を記録
4. 起動時点の repo 状態を baseline として保存
5. child process を PTY 経由で起動
6. stdout / stderr / terminal frames を記録
7. needs input / approval / error を classifier に流す
8. 終了時に session_exited を記録
```

### 11.3 Status / notify

```bash
kagent status --session <id> --state needs_approval --message "Approve pnpm test?"
kagent notify --session <id> --level error --message "backend healthcheck failed"
```

Agent hooks や external scripts から呼べるようにする。

### 11.4 Context

```bash
kagent context --session <id> --markdown
```

v1 以降で、選択中 agent に context を送る機能を検討する。

## 12. 他 Lens の位置付け

v0 の主対象は Agent Lens だが、将来的には以下を追加する。

### 12.1 Worktree Matrix Lens

multi-repo worktree 状態を matrix 表示する。

```text
WORKTREE SET      WEB                     BFF                     BACKEND                 RUNTIME
main              main clean              main clean              main clean              web✓ bff✓ api✓
auth-refactor *   feat/auth Δ8            feat/auth Δ2            main clean              web✓ bff✓ api!
ui-polish         feat/ui Δ3              main clean              main clean              web✓
```

### 12.2 Service Lens

sango services / port / health / logs を見る。

```text
Service  Repo     WorktreeSet    Port  Health  PID    URL
web      web      auth-refactor  3101  OK      18302  localhost
bff      bff      auth-refactor  4101  OK      18344  localhost
backend  backend  auth-refactor  5101  FAIL    18401  localhost
```

### 12.3 Diff Lens

Agent session diff / workspace diff / branch diff を切り替えて見る。

```text
Workspace diff:
  現在の作業ツリー全体

Session diff:
  agent session 開始時点から増えた変更

Branch diff:
  default branch / upstream との差分
```

### 12.4 Logs Lens

Service-centric / Agent-centric の両方を扱う。

Agent Lens から入る場合:

```text
selected agent に関連する services の logs
```

Service Lens から入る場合:

```text
selected service の logs
```

## 13. UI / Visual Design 要件

見た目の詳細検討は別途行う。ただし、企画時点で以下を必須要件として盛り込む。

### 13.1 参考にする体験

Yazi から参考にするもの:

```text
- selection-driven preview
- pane layout
- previewer abstraction
- async background preview
- responsive な terminal UI
- 操作に対する軽快さ
```

lazygit から参考にするもの:

```text
- pane focus
- pane maximize
- あまり見ない pane の縮小
- key help / command surface
- modal 的な action flow
- 情報密度の高い dashboard
```

cmux / Orca から参考にするもの:

```text
- agent tab ごとの状態一覧
- notification / needs input の attention
- agent session の並行監視
```

### 13.2 見た目の設計原則

```text
- terminal 上でも graphical な情報設計をする
- status icon、badge、border、highlight、scrollbar、tab-like header を活用する
- dense だが読みやすい情報密度を目指す
- 選択中対象と非選択対象の contrast を明確にする
- needs input / error は視覚的に即座に分かるようにする
- compact / normal / expanded / maximized を自然に切り替えられるようにする
- 画面幅に応じて layout が破綻しないようにする
```

### 13.3 実装上の禁止事項

```text
- 手作業のスペース埋めによる column alignment
- UI のための ad-hoc string builder
- 画面幅ごとの場当たり的な分岐
- widget 内で外部コマンドを直接叩く
- model と rendering の混在
- snapshot 更新と drawing の混在
```

### 13.4 実装上の必須事項

```text
- Ratatui の layout / widget / style / buffer を前提にする
- component ごとに rendering test を書ける構造にする
- Snapshot と ViewState を分離する
- Keymap を data-driven にする
- Theme token を持つ
- Layout preset を持つ
- Pane state を central store で管理する
```

## 14. 実装フェーズ

### Phase 0: PoC

目的: Agent Lens の核を検証する。

実装:

```text
- Rust project / crate 構成
- kitty provider: list windows, get screen text, focus window
- sango provider: status --json / worktree status --json を読む
- git provider: repo dirty count
- basic Agent Lens
- inferred codex / claude detection
- Enter で kitty window focus
```

成功条件:

```text
- kitty 上の codex / claude window が一覧に出る
- selected window の screen text が preview に出る
- sango service health / port が context に出る
- Enter で該当 window へ移動できる
```

### Phase 1: MVP

目的: tracked session と needs input 監視を成立させる。

実装:

```text
- kagent run wrapper
- session metadata / events / baseline 保存
- PTY capture
- transcript preview
- needs input / approval classifier
- Agent row の 2 行表示
- filter / sort
- context pane compact / expanded
- mark read
```

成功条件:

```text
- kagent run 経由の session が tracked と表示される
- last useful message が row に表示される
- needs input / approval が attention として上位表示される
- selected agent の transcript が中央に表示される
- touched repos / service health が右 pane に出る
```

### Phase 2: sango deep integration

目的: multi-repo / service context を安定化する。

実装:

```text
- sango snapshot --json 対応
- logs tail / cursor 対応
- worktree matrix 対応
- service detail / troubleshoot action
- context markdown generation
```

成功条件:

```text
- web / bff / backend の worktree / branch / diff / service health が安定表示される
- service failure から related logs / related repo / related agent に遷移できる
```

### Phase 3: UI polish / TUI library 化

目的: 見た目と操作感を現代的な TUI として作り込む。

実装:

```text
- theme tokens
- pane transitions
- command palette
- better scroll / selection / preview
- rendering tests
- widget library separation
- visual design iteration
```

成功条件:

```text
- lazygit / yazi に近い操作密度と見た目の完成度になる
- string output tool ではなく TUI application として成立する
- kagent-ui を他の lens / dashboard に再利用できる
```

### Phase 4: Advanced

検討対象:

```text
- kitty tab title / color status sync
- desktop notification
- sango watch --jsonl event-driven update
- context send to selected agent
- limited approve/reject action
- Worktree Matrix Lens / Service Lens / Diff Lens の完成
```

## 15. MVP Acceptance Criteria

MVP の完成基準:

```text
- kitty 上の codex / claude tab/window が Agent Lens に並ぶ
- kagent run 経由の session は tracked と表示される
- direct 起動の session は inferred と表示される
- 各 row に last useful message preview が出る
- NeedsInput / NeedsApproval が attention として上位表示される
- selected agent の conversation preview が中央に出る
- context pane に web / bff / backend の diff count と service health が出る
- backend health fail が agent context に反映される
- Enter で該当 kitty tab/window に focus できる
- pane の compact / expanded / maximized が動作する
- filter / sort / next attention が動作する
- UI は Ratatui component として実装され、manual string margin layout に依存しない
```

## 16. Codex に渡す最初の実装指示

以下を Codex に渡す初回タスクとして使う。

```text
You are implementing a Rust TUI application named kagent.

Goal:
Build the initial skeleton for a kitty-native, sango-aware AI agent dashboard.
The home screen is Agent Lens. It lists AI agent sessions running in kitty tabs/windows, shows each session's status and last message, previews the selected session's conversation/screen, and shows multi-repo/service context from sango.

Hard constraints:
- Do not implement the UI as println/string padding/manual margin output.
- Use a modern TUI architecture based on Ratatui-style widgets, layout constraints, view state, and components.
- Separate model, provider, lens, and rendering logic.
- Treat kitty windows as the internal terminal target, even if the UI says tabs.
- Support tracked and inferred agent sessions.

Initial crates/modules:
- kagent-core: models, snapshot, events, status, selection, lens traits.
- kagent-ui: Agent Lens widgets, layout engine, pane state, keymap.
- kagent-kitty: adapter shell for list/get-text/focus operations. Mockable.
- kagent-sango: adapter shell for reading sango JSON. Mockable.
- kagent-git: repo dirty summary interfaces. Mockable.
- kagent-cli: dash command entry point.

First deliverable:
- A compilable Rust workspace.
- Agent Lens rendered with mock data.
- Three panes: Agents / Conversation Preview / Context Impact.
- Pane states: compact, normal, expanded, maximized.
- Key handling for j/k, Enter, Tab, z, [, ], /, q.
- Mock sessions including NeedsApproval, Streaming, Inferred, Done.
- Tests for core status sorting and view model generation.

Do not start by integrating all external commands.
First build the componentized TUI skeleton with mock Snapshot data.
After the UI skeleton is stable, add kitty and sango providers.
```

## 17. 最終判断

本企画の差別化要素は次の三つである。

```text
1. Agent tab/window ごとの会話プレビューと needs input 監視
2. sango による multi-repo worktree / service / port / health 統合
3. Yazi / lazygit 的な modern TUI としての見た目・操作性
```

特に 1 がホーム画面の主役であり、2 は context として強く結合する。3 は実装品質の最低条件であり、単なる文字列 status dashboard にしてはならない。
