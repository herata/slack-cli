# slack-cli 設計仕様書

- **日付:** 2026-05-14
- **対象:** `slack-cli` MVP（Rust 製 Slack CLI）
- **想定読者:** 実装に着手する coding agent / contributor
- **状態:** ブレインストーミング完了、Plan モード承認済み。実装プランは `writing-plans` スキルで別途作成する

---

## 1. Context

`slack-cli` は **coding agent**（Claude Code / Codex / Cursor 等）から呼ばれることを主目的とする、Slack 操作用のコマンドラインツールである。設計のお手本は [`jira-cli`][jira-cli]：`noun → verb` のサブコマンド構造、`--plain/--json` 切替、TTY 検出による出力フォーマット自動切替、`init` での対話セットアップなど。

**設計の柱:**

1. **Agent friendly first**: pipe 検出時はデフォルトで構造化 JSON、stdout はデータ専用、stderr はログ専用、安定した JSON envelope に `"schema"` を載せて agent が出力スキーマを pin できる。
2. **Elegance over premature abstraction**: workspace 化、`trait SlackClient`、自前 `Render` trait のような「将来必要かもしれない」抽象は **MVP では撤廃**。具象を直接書き、必要になったら抽出する。
3. **Security from day one**: `--token` フラグは作らない、`HeaderValue::set_sensitive(true)`、OS keychain デフォルト、release で `panic="abort"`。
4. **Reproducible toolchain**: `mise` で Rust toolchain と CLI 補助ツールを単一管理（`rust-toolchain.toml` は置かない）。
5. **i18n 方針**: コード・コメント・ログ・エラー・`--help` は **すべて英語**。`README.md`（英語）と `README.ja.md`（日本語）の 2 つを併設。設計仕様書はブレストが日本語なので日本語。

---

## 2. バイナリ名

**`slack-cli`** に決定。公式 Slack CLI（`slack`）との PATH 衝突回避を最優先。

## 3. アーキテクチャ全体像

### 3.1 クレート構成

- **単一クレート**（cargo workspace なし）。
- バイナリ `slack-cli` のみ。ライブラリ公開は MVP では行わない。

### 3.2 async ランタイム

- `#[tokio::main(flavor = "current_thread")]`。CLI は multi-thread 不要、起動最速化。
- `Arc` を持たない設計。`&Ctx { config, client, resolver, out }` を `cmd::*` に参照渡し。Resolver の内部キャッシュを更新する場合は `&mut Ctx` で渡す。

### 3.3 API クライアント

- `reqwest` + `serde` の直叩き。`slack-morphism` などの SDK は採用しない（MVP で必要な API は 15 メソッド前後）。
- `rustls-tls-webpki-roots` を有効化し、OS truststore 非依存・OpenSSL 非依存に。
- `Authorization: Bearer ...` ヘッダは `HeaderValue::set_sensitive(true)` を必須付与し、hyper/reqwest のログ出力時に redact される。
- `with_base_url(uri)` を提供してテスト時の wiremock 差し替えを可能にする（`trait` は切らない）。

### 3.4 エラー設計

- **`anyhow` は使わない**。`Result<T, CliError>` 一本に統一。
- `CliError` は `thiserror::Error` で定義し、`Context { source, hint }` バリアントで context-chain を表現する。
- `#[non_exhaustive]` を付け、将来の variant 追加を非破壊化。
- `exit_code()` / `hint()` メソッドを実装。

#### 3.4.1 exit code 表

| Code | 意味 |
|-----:|------|
| 0 | 成功 |
| 1 | 汎用失敗（ネットワーク、想定外 I/O） |
| 2 | 設定エラー（config パース失敗、workspace 曖昧） |
| 3 | 認証失敗（token 不在、token 無効） |
| 4 | レート制限 |
| 5 | リソース未存在 / 識別子曖昧 |
| 6 | トークン種別不一致 |
| 64 | 引数不正（sysexits `EX_USAGE`） |
| 130 | SIGINT 中断 |

#### 3.4.2 thiserror メッセージ規約

- 小文字開始
- 末尾ピリオドなし
- `"Error:"` の prefix を付けない
- ソースエラーは `#[from]` で吸収

### 3.5 panic ガイドライン

- **lib 層**（`api/`, `resolve.rs`, `config.rs`, `secret.rs`, `id.rs`, `output.rs`）: `unwrap` / `expect` / `panic!` 原則禁止。`const` context のみ許可。
- **bin 層**（`main.rs`, `cmd/*`）: invariant 違反のみ許容。
- `std::panic::set_hook` でクラッシュ時に「This is a bug. Please file an issue.」を stderr に出す。
- **release profile に `panic = "abort"`** を設定（メモリダンプ抑制）。

### 3.6 出力規約

- **stdout = データ専用**、**stderr = ログ・進捗・エラー専用**。pipe で `| jq` してもエラーが混入しない。
- TTY 検出は `std::io::IsTerminal`（標準ライブラリ）。`is-terminal` クレートは使わない。
- `Format::resolve(flag: Option<Format>, stdout_is_tty: bool)`:
  - `flag` が指定されていればそれを採用
  - TTY なら `Table`
  - pipe なら `Json`（agent 主用途のため plain ではなく JSON）
- JSON のインデント: TTY なら pretty、pipe なら compact。
- 色: `--no-color` フラグ / `NO_COLOR` env / 非 TTY のいずれかで disable。

---

## 4. コマンド階層

### 4.1 サブコマンド一覧

```
slack-cli auth        init | whoami | list | logout
slack-cli channel     list | view | create | archive | invite | leave
slack-cli message     list | view | send | reply | update | delete | react
slack-cli search      messages | files
slack-cli user        list | view
slack-cli file        list | upload | download
slack-cli completion  bash | zsh | fish | powershell | elvish
```

- 動詞は `list / view / create / archive / ...` の語彙で揃える（jira-cli と整合）。
- 複数指定可能な引数は positional（例: `slack-cli channel invite C1 U1 U2 U3`）。
- 長フラグは kebab-case。短フラグは jira-cli の慣習に倣う：`-c` config, `-w` workspace, `-v` verbose, `-o` output。
- `-q` は **subcommand 内の `--query` 専用**（`channel list`, `user list`, `search messages`, `search files`）。global の `--quiet` には short flag を付けず、long-only にする（clap は global flag を subcommand に露出させるため short 衝突を避ける）。

### 4.2 グローバルフラグ

`#[arg(global = true)]` で `Cli` トップに **一度だけ flatten**（各 subcommand に flatten は冗長）。

| フラグ | 用途 |
|---|---|
| `-c, --config <PATH>` | 設定ファイルパス（env: `SLACK_CLI_CONFIG`） |
| `-w, --workspace <NAME>` | workspace 切替（env: `SLACK_CLI_WORKSPACE`） |
| `--json` / `--plain` / `--table` | 出力形式強制（排他、`#[group(multiple = false)]`） |
| `-v, --verbose` | `-v`=info / `-vv`=debug / `-vvv`=trace（stderr） |
| `--quiet` | stderr info ログ抑止（short flag は付けない、`-q` は subcommand の `--query` 用） |
| `--no-color` | 色抑止（env: `NO_COLOR` も尊重） |
| `--yes` / `--no-confirm` | 破壊的操作の確認スキップ |
| `--max-results <N>` | 1 ページの取得件数上限（default 50、max 200） |
| `--all` | 全件取得（`--max-pages` と併用） |
| `--max-pages <N>` | `--all` 使用時のページ数上限（default 10） |
| `--dry-run` | write/admin 系で API を呼ばず計画 JSON を出す |

### 4.3 トークン入力ルール

- **`--token` フラグは実装しない**（argv 経由は `ps aux` で漏れる）
- `--token-stdin`: stdin から 1 行受け取る
- `SLACK_CLI_TOKEN` env: CI / agent 向け
- 設定からは TokenStore で読む（後述）
- 入力時に **`xoxp-` / `xoxb-` prefix を validate**し、ミスを早期検出

### 4.4 stdin パイプ規約

- `slack-cli message send <CH> --text -`: stdin の全文をテキストとして送信
- `slack-cli message send <CH> --file -`: stdin から読み込んだ Markdown / blocks 等
- `slack-cli file upload --stdin --filename <NAME>`: stdin からファイル本体を upload

### 4.5 破壊的コマンドの安全弁

`channel archive`, `channel leave`, `message delete`, `auth logout` などに対し:

- **TTY**: 確認 prompt（`Type "yes" to confirm:`）
- **非 TTY**: `--yes` 必須。なければ `CliError::ConfirmationRequired`（exit 64）
- agent からの呼び出しでは prompt が出てこないよう、必ず `--yes` を渡す前提

---

## 5. JSON 出力スキーマ（v0）

### 5.1 envelope

すべての成功時 stdout 出力:

```jsonc
{
  "ok": true,
  "schema": "slack-cli/v0",
  "data": { /* command-specific */ },
  "request_id": "req_..."   // x-slack-req-id があれば
}
```

エラー時 stderr 出力:

```jsonc
{
  "ok": false,
  "schema": "slack-cli/v0",
  "error": {
    "code": "channel_not_found",
    "message": "channel '#nonexistent' not found",
    "hint": { "action": "run", "command": "slack-cli channel list" },
    "exit_code": 5,
    "details": { "kind": "channel", "name": "#nonexistent" },
    "request_id": "req_..."
  }
}
```

### 5.2 list 系

常に `items` + `next_cursor`:

```jsonc
{ "data": { "items": [...], "next_cursor": "dXNlcjpVMDYxTkZUVDI=" } }
```

`next_cursor` が空文字 `""` のときは「これ以上ページなし」。

### 5.3 dry-run（exit 0）

```jsonc
{
  "ok": true,
  "schema": "slack-cli/v0",
  "data": {
    "dry_run": true,
    "would_call": "chat.postMessage",
    "endpoint": "POST /api/chat.postMessage",
    "payload": { "channel": "C012", "text": "hello", "client_msg_id": "..." }
  }
}
```

### 5.4 ambiguous（exit 5）

```jsonc
{
  "ok": false,
  "schema": "slack-cli/v0",
  "error": {
    "code": "ambiguous",
    "message": "ambiguous channel reference 'general' (2 candidates)",
    "exit_code": 5,
    "candidates": [
      { "id": "C001", "name": "general", "kind": "channel", "is_private": false },
      { "id": "C002", "name": "general", "kind": "channel", "is_private": true }
    ]
  }
}
```

### 5.5 hint の構造

自由文字列ではなく構造化：

```jsonc
{ "action": "run",         "command": "slack-cli auth init" }
{ "action": "specify",     "flag": "--workspace" }
{ "action": "list",        "command": "slack-cli channel list" }
```

固定 enum で定義。

### 5.6 whoami 応答

```jsonc
{
  "ok": true,
  "schema": "slack-cli/v0",
  "data": {
    "user_id": "U012",
    "user_name": "alice",
    "team_id": "T012",
    "team_domain": "acme",
    "token_kind": "user",        // "user" | "bot"
    "scopes": ["chat:write", "channels:read", "..."],
    "expires_at": null
  }
}
```

---

## 6. モジュール構成

```
slack-cli/
├─ Cargo.toml
├─ mise.toml                  # rust 1.89 + cargo-nextest + cargo-insta + cargo-deny + git-cliff
├─ deny.toml                  # cargo-deny 設定
├─ cliff.toml                 # git-cliff 設定
├─ .github/
│  ├─ dependabot.yml
│  └─ workflows/{ci.yml, release.yml}
├─ src/
│  ├─ main.rs                 # #[tokio::main(flavor="current_thread")] → ExitCode
│  ├─ logging.rs              # tracing 初期化、EnvFilter で reqwest=warn,hyper=warn 強制下限
│  ├─ cli.rs                  # clap derive、GlobalArgs は #[arg(global = true)] でトップ Cli に一度 flatten
│  ├─ id.rs                   # Newtype: ChannelId / UserId / Ts、enum ChannelRef / UserRef + TryFrom
│  ├─ config.rs               # etcetera + toml_edit + tempfile::NamedTempFile::new_in(config_dir).persist()
│  ├─ secret.rs               # enum TokenStore { Keyring, File, Env } + secrecy::SecretString
│  ├─ output.rs               # enum Format { Table, Plain, Json } + render<T: Tabled+Serialize>()
│  ├─ slack.rs                # reqwest 直叩き、UUID v4 で client_msg_id、Retry-After clamp
│  ├─ resolve.rs              # Resolver: HashMap キャッシュ直所有、&mut self、60s TTL
│  ├─ error.rs                # CliError (thiserror)、#[non_exhaustive]、exit_code()/hint()
│  └─ cmd/
│     ├─ mod.rs               # struct Ctx, pub fn dispatch(cli: Cli) -> Result<(), CliError>
│     ├─ auth.rs              # init / whoami / list / logout
│     ├─ channel.rs           # list / view / create / archive / invite / leave
│     ├─ message.rs           # list / view / send / reply / update / delete / react
│     ├─ search.rs            # messages / files
│     ├─ user.rs              # list / view
│     ├─ file.rs              # list / upload / download
│     └─ completion.rs        # clap_complete で各 shell の completion を生成
├─ tests/
│  ├─ common/mod.rs           # wiremock setup, temp config, helpers（必ず `mod.rs` 命名）
│  ├─ auth.rs
│  ├─ channel.rs
│  ├─ message.rs
│  ├─ search.rs
│  ├─ user.rs
│  ├─ file.rs
│  └─ fixtures/*.json         # Slack API レスポンスのサンプル
├─ README.md                  # 英語 (primary)
├─ README.ja.md                # 日本語
├─ CHANGELOG.md               # git-cliff で生成
└─ docs/superpowers/specs/2026-05-14-slack-cli-design.md
```

---

## 7. 主要型の設計

### 7.1 識別子 Newtype（`src/id.rs`）

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ChannelId(pub(crate) String);
pub struct UserId(pub(crate) String);
pub struct Ts(pub(crate) String);

#[derive(Debug, Clone)]
pub enum ChannelRef { Name(String), Id(ChannelId) }
pub enum UserRef    { Name(String), Id(UserId) }

impl TryFrom<&str> for ChannelRef { /* `#name`, `name`, `C012ABCD` を判別 */ }
impl TryFrom<&str> for UserRef    { /* `@name`, `name`, `U012ABCD` / `W012ABCD` を判別 */ }
```

**判別ルール:**
- 先頭 `#` を剥がせばチャンネル名
- 先頭 `@` を剥がせばユーザー名
- それ以外で `^[CGDUW][A-Z0-9]{8,}$` に合致すれば生 ID
- 上記いずれでもなければ「名前として扱う」（Slack の channel/user 名は ID パターンと衝突しない設計）

### 7.2 TokenStore（`src/secret.rs`）

```rust
pub enum TokenStore { Keyring, File, Env }

impl TokenStore {
    pub async fn load(ws: &WorkspaceConfig) -> Result<SecretString, CliError>;
    pub async fn save(ws: &WorkspaceConfig, token: SecretString) -> Result<(), CliError>;
    pub async fn delete(ws: &WorkspaceConfig) -> Result<(), CliError>;
}
```

**OS 別の振る舞い:**

| OS | Keyring | File | Env |
|---|---|---|---|
| macOS | macOS Keychain (`apple-native`) | 0600 | 普通 |
| Linux (D-Bus 有) | Secret Service (`sync-secret-service`) | 0600 | 普通 |
| Linux (D-Bus 無 = ヘッドレス) | Keyring 試行 → 失敗検知して File にフォールバック + `warn!` ログ | 0600 | 普通 |
| Windows | Credential Manager (`windows-native`) | **無効化（CliError::FileBackendUnsupported）** | 普通 |

**冗長な依存を避けるため、`[target.'cfg(...)'.dependencies]` で OS 別に keyring feature を指定。**

### 7.3 出力 Renderer（`src/output.rs`）

`trait Render` は撤廃。`tabled::Tabled` derive + `serde::Serialize` + `Display` の三点で十分。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[non_exhaustive]
pub enum Format { Table, Plain, Json }

pub fn render<T: tabled::Tabled + serde::Serialize>(
    items: &[T],
    fmt: Format,
    w: &mut dyn std::io::Write,
) -> std::io::Result<()> { /* ... */ }
```

- `Table`: `tabled::Table::new(items)` でヘッダ自動、`tabled::settings::Style::modern_rounded()` で TTY 装飾
- `Plain`: 1 行 1 アイテム、tab 区切り、ヘッダ無し
- `Json`: envelope に包んで `serde_json::to_writer` / `to_writer_pretty`

各 DTO は `#[derive(Tabled, Serialize, Deserialize, Debug, Clone)]` で十分。Tabled の `#[tabled(rename = "...")]` で列名整形、`#[tabled(skip)]` で Plain/Table から除外。

### 7.4 Resolver（`src/resolve.rs`）

```rust
pub struct Resolver {
    cache_channels: HashMap<String, (ChannelId, Instant)>,
    cache_users: HashMap<String, (UserId, Instant)>,
}

impl Resolver {
    pub async fn channel(&mut self, client: &SlackClient, r: &ChannelRef) -> Result<ChannelId, CliError>;
    pub async fn user(&mut self,    client: &SlackClient, r: &UserRef)    -> Result<UserId, CliError>;
}
```

- `&mut self` で borrow を取り、`.await` 跨ぎの内部可変借用問題を回避（`RefCell` は使わない）。
- TTL は **60 秒**、プロセス内のみ、永続化なし。
- `conversations.list` / `users.list` を必要最小限ページングして名前→ID の表を構築。
- 同名複数ヒットは `CliError::Ambiguous { kind, name, candidates }` で exit 5。

### 7.5 SlackClient（`src/slack.rs`）

```rust
pub struct SlackClient {
    base_url: String,     // default: "https://slack.com"
    http: reqwest::Client,
    token: SecretString,
}

impl SlackClient {
    pub fn new(token: SecretString) -> Self;
    pub fn with_base_url(base_url: String, token: SecretString) -> Self;

    pub async fn auth_test(&self) -> Result<AuthTest, CliError>;
    pub async fn post_message(&self, req: PostMessage) -> Result<PostMessageResp, CliError>;
    // ... 15 メソッド前後
}
```

**実装ルール:**

- すべての公開メソッドに `#[tracing::instrument(skip(self, ...with token))]`
- `Authorization` ヘッダは `HeaderValue::from_str(...)?.set_sensitive(true)`（`SecretString::expose_secret()` で生 token を取得）
- レスポンスの `ok: false` を `CliError` の variant にマップ
- `Retry-After` ヘッダを **`min(value, 60)` で clamp**、`ratelimited` のみ **1 回**だけ自動リトライ（指数 backoff なし）
- `client_msg_id` には `uuid::Uuid::new_v4()` を使い、`chat.postMessage` リクエストに自動付与
- `x-slack-req-id` レスポンスヘッダは `CliError::SlackApi { request_id, ... }` に格納し、JSON エラー出力で `request_id` フィールドとして露出

### 7.6 DryRunClient（テスト・dry-run 用）

実装方針：dry-run モードでは **`SlackClient` を呼び出さない**。`cmd::*` の各関数が `if args.dry_run { return render_dry_run(...); }` で早期 return。E2E テストでは wiremock サーバを起動し、`Mock::given(...).expect(0)` で「呼ばれない」ことを assertion。

---

## 8. 設定ファイル

### 8.1 配置

`etcetera::BaseStrategy` の `config_dir()` を使用。

- macOS: `~/Library/Application Support/slack-cli/config.toml`
- Linux: `$XDG_CONFIG_HOME/slack-cli/config.toml`
- Windows: `%APPDATA%\slack-cli\config.toml`

### 8.2 スキーマ

```toml
default_workspace = "acme"

[output]
format = "auto"        # "auto" | "table" | "plain" | "json"
color  = "auto"        # "auto" | "always" | "never"
utc    = false

[workspaces.acme]
team_id      = "T012ABCD"
team_domain  = "acme"
token_source = "keyring"    # "keyring" | "file" | "env"
# token = "xoxp-..."        # token_source = "file" のときのみ
# token_env = "SLACK_TOKEN_ACME"  # token_source = "env" のときのみ

[workspaces.acme.meta]
token_kind = "user"          # "user" | "bot"
user_id    = "U012"
updated_at = "2026-05-14T10:23:00Z"
```

### 8.3 atomic write

- `tempfile::NamedTempFile::new_in(config_dir)` で **同一 FS** にテンポラリを作る（`/tmp` だと cross-device rename 失敗 + 平文残骸リスク）
- 書き込み → `persist(config_path)` で rename
- `toml_edit` を使い、ユーザーが手書きしたコメントを保持

### 8.4 env override

| env | 用途 | 優先度 |
|---|---|---|
| `SLACK_CLI_CONFIG` | 設定ファイルパス | `--config` と同等 |
| `SLACK_CLI_WORKSPACE` | 使用 workspace | `--workspace` と同等 |
| `SLACK_CLI_TOKEN` | トークン直指定 | config より優先 |
| `NO_COLOR` | 色抑止 | `--no-color` と同等 |
| `RUST_LOG` | ログレベル | `--verbose` より弱い |

---

## 9. テスト戦略

### 9.1 3 層

1. **Layer 1: 純粋ユニット** — モジュール末尾の `#[cfg(test)] mod tests`。`id.rs` の TryFrom 全分岐、`output.rs::Format::resolve`、`error.rs::exit_code`、`config.rs` のパース・atomic write 等。
2. **Layer 2: HTTP モック** — `slack.rs` の各 API メソッドを `wiremock` で検証。`MockServer::start()` → `SlackClient::with_base_url(server.uri(), ...)` で差し替え。
3. **Layer 3: E2E** — `tests/{auth,channel,message,...}.rs` に分割。`tests/common/mod.rs` に共通ヘルパ。各テストファイルは Rust の慣習上 **別 crate** として扱われるため、`tests/common/mod.rs` 命名にして共有可能にする（`tests/common.rs` 命名だと自動的にテスト crate 扱いされ警告）。

### 9.2 ツール

- `assert_cmd` + `predicates`: CLI 実行と assertion
- `wiremock`: Slack API 偽装
- `insta` (`json` / `yaml` / `filters` feature): stdout/stderr スナップショット
- `pretty_assertions`: 差分の可読性

### 9.3 snapshot redaction

動的値を `insta::with_settings!` の `filters` で正規化：

```rust
insta::with_settings!({filters => vec![
    (r"\d{10}\.\d{6}", "[TS]"),
    (r"req_[A-Za-z0-9]+", "[REQ]"),
    (r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b", "[UUID]"),
]}, { insta::assert_snapshot!(stdout); });
```

### 9.4 MVP のカバレッジ目標

- 各サブコマンド × （成功 / failure）の **最低 1 件ずつ**の E2E
- HTTP モック層は **全 API メソッド × 200 + 主要エラーコード**
- 純粋ユニットは TryFrom などのテーブル駆動で網羅

### 9.5 dry-run 検証

`Mock::given(method("POST")).respond_with(...).expect(0)` で「呼ばれない」ことを wiremock に assertion させる。`expect(0)` を全 path に張れば確実。

---

## 10. CI / リリース

### 10.1 `mise.toml`

```toml
[tools]
rust = "1.89"
"cargo:cargo-nextest" = "0.9"
"cargo:cargo-insta" = "1"
"cargo:cargo-deny" = "0.16"
"cargo:git-cliff" = "2"
```

### 10.2 GitHub Actions

#### `.github/workflows/ci.yml`

- **3 OS マトリクス**（Linux/macOS/Windows）で：
  - `cargo fmt --check`（Linux 1 つのみ）
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo nextest run`
  - `cargo doc --no-deps --document-private-items`（Linux 1 つのみ）
- **`linux-headless` ジョブ**: Docker container（D-Bus 無し）で keyring → file fallback の integration test
- **`msrv` ジョブ**: Rust 1.89 ピン留めで `cargo check`
- **`cargo deny check advisories bans licenses sources`** を全 OS で
- **すべての action を SHA pin**（Dependabot 設定で自動更新）
- `permissions: contents: read` をデフォルト、リリース job のみ `contents: write`
- `pull_request_target` は使用禁止、`pull_request` のみ

#### `.github/dependabot.yml`

- `package-ecosystem: github-actions` を週 1 更新
- `package-ecosystem: cargo` を週 1 更新

#### `.github/workflows/release.yml`

- タグ `v*` push でトリガー
- `taiki-e/upload-rust-binary-action`（**SHA pin**）で 6 ターゲットビルド：
  - `x86_64-unknown-linux-musl` / `aarch64-unknown-linux-musl`
  - `x86_64-apple-darwin` / `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc` / `aarch64-pc-windows-msvc`
- `checksum: sha256` で **SHA256SUMS 自動公開**
- changelog は `git-cliff` で conventional commits から自動生成
- release notes は **英語のみ**

### 10.3 リリースプロファイル

```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"
opt-level = "z"
panic = "abort"
```

---

## 11. 依存クレート

### 11.1 ランタイム

| クレート | 主な features / 目的 |
|---|---|
| `clap` | `derive`, `wrap_help` |
| `tokio` | `macros`, `rt`, `fs`, `io-util`, `signal` (current_thread) |
| `reqwest` | `json`, `rustls-tls-webpki-roots`, `multipart`, `stream`、`default-features = false` |
| `serde` / `serde_json` | DTO |
| `toml_edit` | 設定の atomic 編集（コメント保持） |
| `thiserror` | エラー定義（`anyhow` は使わない） |
| `tracing` / `tracing-subscriber` | ログ、`env-filter` で `reqwest=warn,hyper=warn` を強制 |
| `etcetera` | XDG 準拠の設定パス解決 |
| `keyring` (v3) | OS 別 feature を `[target.'cfg(...)'.dependencies]` で指定 |
| `secrecy` 0.10 | `SecretString`, `ExposeSecret` |
| `tabled` | `Tabled` derive |
| `owo-colors` | `supports-colors` feature |
| `tempfile` | atomic write |
| `indicatif` | file upload 進捗（stderr） |
| `rpassword` | `auth init` のトークン入力（echo 抑止） |
| `uuid` | `v4` feature、冪等性キー |
| `clap_complete` | shell completion 生成 |

### 11.2 dev

`assert_cmd`, `predicates`, `wiremock`, `insta` (`json`, `yaml`, `filters`), `pretty_assertions`

### 11.3 lint 設定（crate root）

```rust
#![warn(missing_debug_implementations, rust_2018_idioms, unreachable_pub)]
#![warn(clippy::pedantic)]
// nursery / cargo は不採用（ノイズ大）
```

---

## 12. agent UX 上の重要規約

1. **JSON envelope** に `"schema": "slack-cli/v0"` を必ず含める。0.x は breaking 許容、agent 側はこのフィールドで pin する。
2. **list 系は常に `{ items, next_cursor }` 形**。単一 / 配列の揺れを排除。
3. **null は省略しない**: 「フィールド有るが値 null」と「フィールド無し」を区別。常に存在 + null。
4. **デフォルト件数 50 / max 200**。`--all` は `--max-pages` の上限（default 10）を伴う。安全弁。
5. **error hint** は構造化（`{ "action": "...", "command": "..." }`）。自由文字列は避ける。
6. **`--dry-run` の出力** は `{ "dry_run": true, "would_call": ..., "endpoint": ..., "payload": ... }` 固定形。
7. **`whoami` で `scopes` / `token_kind` を返す**: agent が起動時に実行可能 API を判定できる。
8. **--help の `long_about`** に各 subcommand で **JSON 出力例 + 典型 invocation 2-3 個** を含める。agent の few-shot 学習を助ける。
9. **`message react` の `already_reacted` 等**を Slack 側のエラーから success にマップ。
10. **request_id**（`x-slack-req-id`）を成功時もエラー時も JSON envelope に含める。

---

## 13. セキュリティ規約

1. **`--token` フラグは作らない**。argv 経由は禁止。
2. **`HeaderValue::set_sensitive(true)`** を Authorization に必須。
3. **`#[tracing::instrument(skip(token, ...))]`** で token を span field に乗せない。
4. **`tracing-subscriber::EnvFilter`** の default を `slack_cli=info,reqwest=warn,hyper=warn` にし、ユーザーが `RUST_LOG` で緩めても hyper のトレース dump を出さない。
5. **release profile に `panic = "abort"`** でメモリダンプ抑制。
6. **OS keychain がデフォルト**、File backend は `0600` 強制、Windows は File backend 無効。
7. **Linux ヘッドレス**: keyring 失敗を検知して File に自動 fallback + warn。
8. **`xoxp-` / `xoxb-` prefix を validate**してミス検出。
9. **SHA256SUMS をリリースに自動公開**（codesign / notarize は MVP では非対応、README で明示）。
10. **Slack エラーの redact**: `response_metadata` の生 dump は出さず、code と最小限の details に絞る。

---

## 14. ロギング

- 出力先: **stderr**（`tracing_subscriber::fmt().with_writer(std::io::stderr)`）
- フォーマット: human-friendly（`with_target(false).with_level(true)`）
- `EnvFilter`: default `slack_cli=info,reqwest=warn,hyper=warn`
- `--verbose` フラグで上書き（`-v`/`-vv`/`-vvv` → info/debug/trace）
- `--quiet` で `EnvFilter` を `error` に絞る

---

## 15. このスペックで作成する MVP 範囲（非ゴール）

**含まない:**

- cargo workspace 化
- MCP サーバモード
- Block Kit インタラクティブエディタ
- batch 操作（JSON Lines 入力で複数メッセージ）
- mise community plugin、Homebrew tap、crates.io 公開
- macOS codesign / notarization、Windows EV cert 署名
- Socket Mode / Events API
- ベンチマーク、fuzz
- JSON `--json-strict`（schema 変更時の strict mode）

これらは post-MVP で再検討する。

---

## 16. 検証チェックリスト

- [ ] `mise install` で toolchain がセットアップできる
- [ ] `cargo check` がクリーンに通る（最小 `src/main.rs` を入れた段階で）
- [ ] `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` 通る
- [ ] `cargo nextest run` 通る
- [ ] `cargo deny check advisories bans licenses sources` 通る
- [ ] `--help` 出力が英語、`--version` が正しい
- [ ] `slack-cli auth init` の対話フローが動く
- [ ] `slack-cli auth whoami --json` で JSON envelope が出る
- [ ] `slack-cli channel list --json` の出力が JSON Lines ではなく envelope ＋ items
- [ ] `slack-cli message send --dry-run` で wiremock に何も飛ばない
- [ ] `slack-cli completion bash` で valid な bash completion が出る
- [ ] keyring 無効の Linux container で File backend に fallback する

---

## 付録 A: 用語

- **workspace**: `slack-cli` 設定上の Slack workspace（team）の論理単位。`acme` や `personal` のような名前で参照する。
- **TokenStore**: トークンの保存方式 (`Keyring` / `File` / `Env`) を選ぶ抽象。
- **identifier resolve**: `#channel-name` や `@username` のような人間可読参照を内部 ID（`Cxxxx` / `Uxxxx`）に変換する処理。

## 付録 B: 参照

- [jira-cli][jira-cli]: コマンド階層の参考
- [Slack Web API][slack-api]: 対応 API
- [`mise`][mise]: toolchain 管理
- [`tabled`][tabled] / [`tracing-subscriber`][tracing] / [`wiremock`][wiremock]: 主要クレート

[jira-cli]: https://github.com/ankitpokhrel/jira-cli
[slack-api]: https://api.slack.com/methods
[mise]: https://mise.jdx.dev
[tabled]: https://crates.io/crates/tabled
[tracing]: https://crates.io/crates/tracing-subscriber
[wiremock]: https://crates.io/crates/wiremock
